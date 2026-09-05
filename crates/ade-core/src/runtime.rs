//! M1 runtime: own thread + tokio loop driving `opencode serve`.
//!
//! UI thread sends [`Command`]s; the loop mutates a [`Store`] and publishes
//! `Arc<Store>` snapshots. SSE is best-effort — every `Connected` frame and
//! every poll tick reconciles via REST.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use opencode_codes::client_async::OpencodeClient;
use opencode_codes::protocol_generated::types::{
    Event, PermissionReplyParams, PermissionReplyResponse, PromptAsyncParams,
    PromptAsyncParamsPartsItem, Session, SessionCreateParams, SessionStatus, SubtaskPartInputModel,
    TextPartInput, Todo,
};
use opencode_codes::sse::{RetryConfig, StreamEvent};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::config::AppConfig;
use crate::server::AdeServer;
use crate::state::{ConnState, MessageEntry, SelectedModel, Store};

#[derive(Debug, Clone)]
pub enum Command {
    CreateSession {
        title: String,
    },
    SelectSession {
        id: String,
    },
    Prompt {
        text: String,
    },
    Abort,
    FetchDiff(String),
    PermissionReply {
        permission_id: String,
        response: PermissionResponse,
    },
    SetModel {
        provider_id: String,
        model_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    Once,
    Always,
    Reject,
}

impl PermissionResponse {
    pub fn as_wire(&self) -> PermissionReplyResponse {
        match self {
            Self::Once => PermissionReplyResponse::Once,
            Self::Always => PermissionReplyResponse::Always,
            Self::Reject => PermissionReplyResponse::Reject,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeHandle {
    tx: UnboundedSender<Command>,
    slot: Arc<RwLock<Arc<Store>>>,
}

impl RuntimeHandle {
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    pub fn snapshot(&self) -> Arc<Store> {
        self.slot.read().map(|g| Arc::clone(&g)).unwrap_or_default()
    }
}

pub fn spawn(config: AppConfig) -> RuntimeHandle {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let slot: Arc<RwLock<Arc<Store>>> = Arc::new(RwLock::new(Arc::new(Store::default())));
    let handle = RuntimeHandle {
        tx,
        slot: Arc::clone(&slot),
    };

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime for ADE");
        rt.block_on(outer_loop(config, rx, slot));
    });

    handle
}

async fn outer_loop(
    config: AppConfig,
    mut rx: UnboundedReceiver<Command>,
    slot: Arc<RwLock<Arc<Store>>>,
) {
    loop {
        match AdeServer::start(&config).await {
            Ok(server) => {
                {
                    let mut st = LoopState::load(&slot);
                    st.store.conn = ConnState::Connected;
                    publish(&slot, &st);
                }
                run_session(&config, server, &mut rx, &slot).await;
                // run_session only returns on fatal stream/setup failure: retry.
                let mut st = LoopState::load(&slot);
                st.store.conn = ConnState::Disconnected;
                st.store.push_error("server loop ended — reconnecting");
                publish(&slot, &st);
            }
            Err(e) => {
                let mut st = LoopState::load(&slot);
                st.store.conn = ConnState::Disconnected;
                st.store
                    .push_error(format!("opencode serve failed to start: {e:#}"));
                publish(&slot, &st);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

struct LoopState {
    store: Store,
}

impl LoopState {
    fn load(slot: &RwLock<Arc<Store>>) -> Self {
        let store = slot.read().map(|g| (**g).clone()).unwrap_or_default();
        Self { store }
    }
}

fn publish(slot: &RwLock<Arc<Store>>, st: &LoopState) {
    if let Ok(mut guard) = slot.write() {
        *guard = Arc::new(st.store.clone());
    }
}

async fn run_session(
    config: &AppConfig,
    server: AdeServer,
    rx: &mut UnboundedReceiver<Command>,
    slot: &Arc<RwLock<Arc<Store>>>,
) {
    let client = server.client.clone();
    let mut st = LoopState::load(slot);

    bootstrap(&mut st, &client).await;
    publish(slot, &st);

    // SSE pump: applies events live; reconciles on every (re)connect.
    let pump_client = client.clone();
    let pump_slot = Arc::clone(slot);
    tokio::spawn(async move {
        sse_pump(pump_client, pump_slot).await;
    });

    let mut poll = tokio::time::interval(Duration::from_millis(config.poll_interval_ms));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut tick: u64 = 0;

    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { return }; // UI gone: end session loop.
                if !handle_command(&mut st, &client, cmd).await {
                    return;
                }
                publish(slot, &st);
            }
            _ = poll.tick() => {
                tick += 1;
                poll_once(&mut st, &client, tick).await;
                publish(slot, &st);
            }
        }
    }
}

async fn sse_pump(client: OpencodeClient, slot: Arc<RwLock<Arc<Store>>>) {
    let retry = RetryConfig {
        initial_interval: Duration::from_millis(500),
        max_interval: Duration::from_secs(10),
        factor: 1.5,
        max_retries: None,
    };
    let mut stream = match client.event_stream(retry) {
        Ok(s) => s,
        Err(e) => {
            let mut st = LoopState::load(&slot);
            st.store.push_error(format!("SSE subscribe failed: {e:#}"));
            publish(&slot, &st);
            return;
        }
    };
    while let Some(item) = stream.next().await {
        match item {
            Ok(StreamEvent::Connected) => {
                let mut st = LoopState::load(&slot);
                if let Some(sid) = st.store.active_session.clone() {
                    reconcile_messages(&mut st, &client, &sid).await;
                }
                publish(&slot, &st);
            }
            Ok(StreamEvent::Event(ev)) => {
                apply_stream_event(&client, &slot, *ev).await;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::debug!("SSE frame error: {e:#}");
            }
        }
    }
    // Stream ended (server gone): mark disconnected so outer loop reconnects.
    let mut st = LoopState::load(&slot);
    st.store.conn = ConnState::Disconnected;
    st.store.push_error("event stream ended");
    publish(&slot, &st);
}

async fn apply_stream_event(client: &OpencodeClient, slot: &Arc<RwLock<Arc<Store>>>, ev: Event) {
    let mut st = LoopState::load(slot);
    // Permission + todo frames carry full data — no extra fetch needed.
    // Message frames only patch the mirror; the poll tick reconciles fully.
    st.store.apply_event(&ev);
    if matches!(
        ev,
        Event::SessionCreated(_) | Event::SessionDeleted(_) | Event::SessionIdle(_)
    ) {
        reconcile_sessions(&mut st, client).await;
    }
    publish(slot, &st);
}

async fn poll_once(st: &mut LoopState, client: &OpencodeClient, tick: u64) {
    reconcile_sessions(st, client).await;
    if let Some(sid) = st.store.active_session.clone() {
        reconcile_messages(st, client, &sid).await;
        fetch_todos(st, client, &sid).await;
    }
    if tick.is_multiple_of(10) {
        fetch_providers(st, client).await;
    }
}

/// Returns false when the session loop should end (currently never — all
/// commands are recoverable).
async fn handle_command(st: &mut LoopState, client: &OpencodeClient, cmd: Command) -> bool {
    match cmd {
        Command::CreateSession { title } => {
            let params = SessionCreateParams {
                agent: None,
                metadata: None,
                model: None,
                parent_id: None,
                permission: None,
                title: Some(title).filter(|t| !t.trim().is_empty()),
                workspace_id: None,
            };
            match client.create_session(&params).await {
                Ok(s) => {
                    st.store.active_session = Some(s.id.clone());
                    st.store.sessions.insert(s.id.clone(), s);
                    if let Some(sid) = st.store.active_session.clone() {
                        reconcile_messages(st, client, &sid).await;
                    }
                }
                Err(e) => st.store.push_error(format!("create session failed: {e:#}")),
            }
        }
        Command::SelectSession { id } => {
            if st.store.sessions.contains_key(&id) {
                st.store.active_session = Some(id.clone());
                reconcile_messages(st, client, &id).await;
                fetch_todos(st, client, &id).await;
            }
        }
        Command::Prompt { text } => prompt(st, client, text).await,
        Command::Abort => {
            if let Some(sid) = st.store.active_session.clone()
                && let Err(e) = client.abort(&sid).await
            {
                st.store.push_error(format!("abort failed: {e:#}"));
            }
        }
        Command::PermissionReply {
            permission_id,
            response,
        } => reply_permission(st, client, permission_id, response).await,
        Command::FetchDiff(sid) => {
            let path = format!("/session/{sid}/diff");
            match client
                .request::<serde_json::Value>(reqwest::Method::GET, &path, None)
                .await
            {
                Ok(v) => {
                    st.store.diffs.insert(sid, v);
                }
                Err(e) => st.store.push_error(format!("diff fetch failed: {e:#}")),
            }
        }
        Command::SetModel {
            provider_id,
            model_id,
        } => {
            st.store.selected_model = Some(SelectedModel {
                provider_id,
                id: model_id,
            });
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
        agent: None,
        format: None,
        message_id: None,
        model: st
            .store
            .selected_model
            .as_ref()
            .map(|m| SubtaskPartInputModel {
                provider_id: m.provider_id.clone(),
                model_id: m.id.clone(),
            }),
        no_reply: None,
        parts: vec![PromptAsyncParamsPartsItem::Text(TextPartInput {
            id: None,
            ignored: None,
            metadata: None,
            synthetic: None,
            text,
            time: None,
            type_: String::new(),
        })],
        system: None,
        tools: None,
        variant: None,
    };
    match client.prompt_async(&sid, &params).await {
        Err(e) => {
            st.store.push_error(format!("prompt failed: {e:#}"));
        }
        Ok(()) => {
            // Optimistically mark busy; authoritative status arrives via SSE/poll.
            st.store.statuses.insert(sid, SessionStatus::Busy);
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
        Err(e) => st
            .store
            .push_error(format!("permission reply failed: {e:#}")),
    }
}

async fn reconcile_messages(st: &mut LoopState, client: &OpencodeClient, sid: &str) {
    match client.list_messages(sid).await {
        Ok(msgs) => {
            st.store
                .set_messages(sid, msgs.into_iter().map(MessageEntry::from).collect());
        }
        Err(e) => st.store.push_error(format!("list messages failed: {e:#}")),
    }
}

async fn reconcile_sessions(st: &mut LoopState, client: &OpencodeClient) {
    match client
        .request::<Vec<Session>>(reqwest::Method::GET, "/session", None)
        .await
    {
        Ok(sessions) => {
            for s in sessions {
                if st.store.active_session.is_none() {
                    st.store.active_session = Some(s.id.clone());
                }
                st.store.sessions.insert(s.id.clone(), s);
            }
        }
        Err(e) => tracing::debug!("session list refresh failed: {e:#}"),
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
        Err(e) => tracing::debug!("todos unavailable: {e:#}"),
    }
}

async fn bootstrap(st: &mut LoopState, client: &OpencodeClient) {
    reconcile_sessions(st, client).await;
    fetch_providers(st, client).await;
}

/// Parse `GET /provider` defensively — the shape is not in the wrapped spec.
async fn fetch_providers(st: &mut LoopState, client: &OpencodeClient) {
    #[derive(serde::Deserialize)]
    struct ProviderRaw {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        models: Option<serde_json::Value>,
    }

    #[derive(serde::Deserialize)]
    struct ProvidersEnvelope {
        #[serde(default)]
        providers: Vec<ProviderRaw>,
    }

    let parsed: Result<Vec<crate::state::ProviderInfo>, anyhow::Error> = async {
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
        Err(e) => st
            .store
            .push_error(format!("providers fetch failed: {e:#}")),
    }
}
