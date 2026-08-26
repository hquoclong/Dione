//! The background runtime: owns the tokio loop that drives the opencode
//! server, the SSE pump, and REST reconciliation. The UI talks to it through
//! [`Command`]s and reads immutable [`Store`] snapshots.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context as _, Result};
use opencode_codes::client_async::OpencodeClient;
use opencode_codes::protocol_generated::types::{
    MessageWithParts, PermissionReplyParams, PermissionReplyResponse, PromptAsyncParams,
    PromptAsyncParamsPartsItem, SessionCreateParams, SessionCreateParamsModel, SessionStatus,
    SubtaskPartInputModel, TextPartInput, Todo,
};
use opencode_codes::sse::{RetryConfig, StreamEvent};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::config::AppConfig;
use crate::server::AdeServer;
use crate::state::{MessageEntry, Store};

#[derive(Debug, Clone)]
pub enum PermissionResponse {
    Once,
    Always,
    Reject,
}

impl PermissionResponse {
    fn as_wire(&self) -> PermissionReplyResponse {
        match self {
            Self::Once => PermissionReplyResponse::Once,
            Self::Always => PermissionReplyResponse::Always,
            Self::Reject => PermissionReplyResponse::Reject,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    CreateSession {
        title: String,
    },
    SelectSession(String),
    DeleteSession(String),
    ForkSession(String),
    SetModel {
        provider_id: String,
        model_id: String,
    },
    Prompt {
        text: String,
    },
    Abort,
    PermissionReply {
        permission_id: String,
        response: PermissionResponse,
    },
    FetchDiff(String),
}

/// Clone-able handle for the UI thread.
#[derive(Clone)]
pub struct RuntimeHandle {
    cmd_tx: mpsc::UnboundedSender<Command>,
    slot: Arc<RwLock<Arc<Store>>>,
}

impl RuntimeHandle {
    pub fn send(&self, cmd: Command) {
        if let Err(e) = self.cmd_tx.send(cmd) {
            tracing::error!("runtime gone: {e}");
        }
    }

    /// Latest immutable snapshot. Cheap Arc clone.
    pub fn snapshot(&self) -> Arc<Store> {
        self.slot.read().expect("store slot poisoned").clone()
    }
}

struct LoopState {
    store: Store,
    selected_model: Option<SessionCreateParamsModel>,
}

/// Spawn the runtime on a dedicated OS thread with its own tokio runtime.
pub fn spawn(config: AppConfig) -> RuntimeHandle {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let slot = Arc::new(RwLock::new(Arc::new(Store::default())));

    let slot_for_thread = slot.clone();
    std::thread::Builder::new()
        .name("ade-runtime".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("tokio runtime");
            let result = rt.block_on(main_loop(config, cmd_rx, slot_for_thread));
            if let Err(e) = result {
                tracing::error!("runtime loop terminated: {e:#}");
            }
        })
        .expect("spawn ade-runtime thread");

    RuntimeHandle { cmd_tx, slot }
}

async fn main_loop(
    config: AppConfig,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    slot: Arc<RwLock<Arc<Store>>>,
) -> Result<()> {
    let mut st = LoopState {
        store: Store::default(),
        selected_model: None,
    };

    let server = match AdeServer::start(&config).await {
        Ok(s) => s,
        Err(e) => {
            st.store.conn = crate::state::ConnState::Disconnected(format!("{e:#}"));
            publish(&slot, &st);
            return Err(e);
        }
    };
    let client = server.client.clone();

    // Subscribe BEFORE any activity so early frames are less likely missed.
    let mut events = client
        .event_stream(RetryConfig {
            max_retries: None,
            ..RetryConfig::default()
        })
        .context("opening SSE stream")?;

    bootstrap(&mut st, &client).await;
    publish(&slot, &st);
    tracing::info!("runtime up at {}", server.base_url);

    let mut poll_count: u64 = 0;
    let mut shutdown_server = Some(server);

    loop {
        let busy = st.store.is_busy();
        let delay = Duration::from_millis(if busy {
            config.busy_poll_interval_ms
        } else {
            config.poll_interval_ms
        });

        tokio::select! {
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else {
                    tracing::info!("command channel closed; shutting down");
                    break;
                };
                if !handle_command(&mut st, &client, cmd).await {
                    break;
                }
            }
            item = events.next() => {
                match item {
                    Some(Ok(StreamEvent::Connected)) => {
                        st.store.conn = crate::state::ConnState::Connected;
                        reconcile_sessions(&mut st, &client).await;
                        if let Some(sid) = st.store.active_session.clone() {
                            reconcile_messages(&mut st, &client, &sid).await;
                        }
                    }
                    Some(Ok(StreamEvent::Event(ev))) => {
                        st.store.apply_event(&ev);
                        if let EventNeedsReconcile::Yes(sid) = event_reconcile_hint(&ev) {
                            reconcile_messages(&mut st, &client, &sid).await;
                        }
                    }
                    Some(Ok(StreamEvent::Unknown(raw))) => {
                        tracing::debug!("unknown event type: {}", raw.get("type").and_then(|t| t.as_str()).unwrap_or("?"));
                    }
                    Some(Err(e)) => {
                        st.store.conn = crate::state::ConnState::Disconnected(e.to_string());
                        // Stream ends when retry budget is exhausted; recreate it.
                        events = client.event_stream(RetryConfig {
                            max_retries: None,
                            ..RetryConfig::default()
                        })?;
                    }
                    // StreamEvent is #[non_exhaustive]; tolerate new variants.
                    Some(Ok(_)) => {}
                    None => {
                        events = client.event_stream(RetryConfig {
                            max_retries: None,
                            ..RetryConfig::default()
                        })?;
                    }
                }
            }
            _ = tokio::time::sleep(delay) => {
                poll_count += 1;
                // Periodic safety net independent of SSE health.
                if busy
                    && let Some(sid) = st.store.active_session.clone() {
                        reconcile_messages(&mut st, &client, &sid).await;
                    }
                if poll_count.is_multiple_of(10) {
                    reconcile_sessions(&mut st, &client).await;
                }
            }
        }

        st.store.recompute_totals();
        publish(&slot, &st);
    }

    if let Some(s) = shutdown_server.take() {
        s.shutdown().await;
    }
    Ok(())
}

enum EventNeedsReconcile {
    Yes(String),
    No,
}

fn event_reconcile_hint(
    ev: &opencode_codes::protocol_generated::types::Event,
) -> EventNeedsReconcile {
    use opencode_codes::protocol_generated::types::Event::*;
    match ev {
        SessionIdle(e) => EventNeedsReconcile::Yes(e.properties.session_id.clone()),
        _ => EventNeedsReconcile::No,
    }
}

async fn handle_command(st: &mut LoopState, client: &OpencodeClient, cmd: Command) -> bool {
    match cmd {
        Command::CreateSession { title } => {
            let params = SessionCreateParams {
                title: (!title.is_empty()).then_some(title),
                model: st.selected_model.clone(),
                ..Default::default()
            };
            match client.create_session(&params).await {
                Ok(session) => {
                    let sid = session.id.clone();
                    st.store.sessions.insert(sid.clone(), session);
                    st.store.active_session = Some(sid);
                }
                Err(e) => st.store.push_error(format!("create session failed: {e}")),
            }
        }
        Command::SelectSession(sid) => {
            st.store.active_session = Some(sid.clone());
            reconcile_messages(st, client, &sid).await;
            fetch_todos(st, client, &sid).await;
        }
        Command::DeleteSession(sid) => {
            let path = format!("/session/{sid}");
            match client
                .request_unit(reqwest::Method::DELETE, &path, None)
                .await
            {
                Ok(()) => {
                    st.store.sessions.remove(&sid);
                    st.store.messages.remove(&sid);
                    st.store.statuses.remove(&sid);
                    if st.store.active_session.as_deref() == Some(sid.as_str()) {
                        st.store.active_session = None;
                    }
                }
                Err(e) => st.store.push_error(format!("delete session failed: {e}")),
            }
        }
        Command::ForkSession(sid) => match client.fork_session(&sid).await {
            Ok(new_session) => {
                let nid = new_session.id.clone();
                st.store.sessions.insert(nid.clone(), new_session);
                st.store.active_session = Some(nid.clone());
                reconcile_messages(st, client, &nid).await;
            }
            Err(e) => st.store.push_error(format!("fork failed: {e}")),
        },
        Command::SetModel {
            provider_id,
            model_id,
        } => {
            st.selected_model = Some(SessionCreateParamsModel {
                provider_id,
                id: model_id,
                variant: None,
            });
        }
        Command::Prompt { text } => {
            prompt(st, client, text).await;
        }
        Command::Abort => {
            if let Some(sid) = st.store.active_session.clone()
                && let Err(e) = client.abort(&sid).await
            {
                st.store.push_error(format!("abort failed: {e}"));
            }
        }
        Command::PermissionReply {
            permission_id,
            response,
        } => {
            reply_permission(st, client, permission_id, response).await;
        }
        Command::FetchDiff(sid) => {
            let path = format!("/session/{sid}/diff");
            match client
                .request::<serde_json::Value>(reqwest::Method::GET, &path, None)
                .await
            {
                Ok(v) => {
                    st.store.diffs.insert(sid, v);
                }
                Err(e) => st.store.push_error(format!("diff fetch failed: {e}")),
            }
        }
    }
    true
}

async fn prompt(st: &mut LoopState, client: &OpencodeClient, text: String) {
    let Some(sid) = st.store.active_session.clone() else {
        st.store.push_error("no active session — create one first");
        return;
    };
    let params = PromptAsyncParams {
        parts: vec![PromptAsyncParamsPartsItem::Text(TextPartInput {
            text,
            ..Default::default()
        })],
        model: st.selected_model.as_ref().map(|m| SubtaskPartInputModel {
            provider_id: m.provider_id.clone(),
            model_id: m.id.clone(),
        }),
        ..Default::default()
    };
    match client.prompt_async(&sid, &params).await {
        Err(e) => {
            st.store.push_error(format!("prompt failed: {e}"));
        }
        _ => {
            // Optimistically mark busy; authoritative status arrives via SSE/poll.
            st.store.statuses.insert(sid.clone(), SessionStatus::Busy);
        }
    }
}

async fn reply_permission(
    st: &mut LoopState,
    client: &OpencodeClient,
    permission_id: String,
    response: PermissionResponse,
) {
    let pending = st.store.pending_permissions.get(&permission_id).cloned();
    let Some(pending) = pending else {
        st.store.push_error("reply for unknown permission");
        return;
    };
    let params = PermissionReplyParams {
        response: response.as_wire(),
    };
    match client
        .respond_permission(&pending.session_id, &permission_id, &params)
        .await
    {
        Ok(_) => {
            st.store.pending_permissions.remove(&permission_id);
        }
        Err(e) => st.store.push_error(format!("permission reply failed: {e}")),
    }
}

async fn reconcile_messages(st: &mut LoopState, client: &OpencodeClient, sid: &str) {
    match client.list_messages(sid).await {
        Ok(msgs) => {
            st.store
                .set_messages(sid, msgs.into_iter().map(entry_from).collect());
        }
        Err(e) => st.store.push_error(format!("list messages failed: {e}")),
    }
}

fn entry_from(mwp: MessageWithParts) -> MessageEntry {
    MessageEntry {
        info: mwp.info,
        parts: mwp.parts,
    }
}

async fn reconcile_sessions(st: &mut LoopState, client: &OpencodeClient) {
    match client
        .request::<Vec<opencode_codes::protocol_generated::types::Session>>(
            reqwest::Method::GET,
            "/session",
            None,
        )
        .await
    {
        Ok(sessions) => {
            let fresh: std::collections::BTreeMap<String, _> =
                sessions.into_iter().map(|s| (s.id.clone(), s)).collect();
            // Keep locally-created sessions the server may not have echoed yet.
            for (id, s) in fresh {
                st.store.sessions.insert(id, s);
            }
        }
        Err(e) => tracing::debug!("session list refresh failed: {e}"),
    }
}

async fn fetch_todos(st: &mut LoopState, client: &OpencodeClient, sid: &str) {
    let path = format!("/session/{sid}/todo");
    match client
        .request::<Vec<Todo>>(reqwest::Method::GET, &path, None)
        .await
    {
        Ok(todos) => {
            st.store.todos.insert(sid.to_string(), todos);
        }
        Err(e) => tracing::debug!("todos unavailable: {e}"),
    }
}

async fn bootstrap(st: &mut LoopState, client: &OpencodeClient) {
    reconcile_sessions(st, client).await;
    fetch_providers(st, client).await;
}

/// Parse `GET /provider` defensively — the shape is not in the wrapped spec.
async fn fetch_providers(st: &mut LoopState, client: &OpencodeClient) {
    #[derive(Deserialize)]
    struct ProviderRaw {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        models: Option<serde_json::Value>,
    }

    #[derive(Deserialize)]
    struct ProvidersEnvelope {
        #[serde(default)]
        providers: Vec<ProviderRaw>,
    }

    let parsed: Result<Vec<crate::state::ProviderInfo>> = async {
        let raw: serde_json::Value = client
            .request(reqwest::Method::GET, "/provider", None)
            .await?;
        let list: Vec<ProviderRaw> = if raw.is_array() {
            serde_json::from_value(raw)?
        } else {
            serde_json::from_value::<ProvidersEnvelope>(raw)?.providers
        };
        Ok(list
            .into_iter()
            .map(|p| {
                let mut models = Vec::new();
                if let Some(obj) = p.models.as_ref().and_then(|m| m.as_object()) {
                    for (model_id, mv) in obj {
                        let name = mv
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or(model_id)
                            .to_string();
                        models.push((model_id.clone(), name));
                    }
                }
                crate::state::ProviderInfo {
                    provider_id: p.id,
                    provider_name: p.name.unwrap_or_default(),
                    models,
                }
            })
            .collect())
    }
    .await;

    match parsed {
        Ok(providers) => st.store.providers = providers,
        Err(e) => st.store.push_error(format!("providers fetch failed: {e}")),
    }
}

fn publish(slot: &RwLock<Arc<Store>>, st: &LoopState) {
    if let Ok(mut guard) = slot.write() {
        *guard = Arc::new(st.store.clone());
    }
}
