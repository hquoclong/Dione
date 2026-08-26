//! The state mirror. Single source of truth for the UI.
//!
//! Events from the SSE stream mutate this incrementally; REST reconciliation
//! replaces per-session messages wholesale. Everything is `Clone` so the
//! runtime loop can publish immutable snapshots.

use std::collections::{BTreeMap, HashMap, VecDeque};

use opencode_codes::protocol_generated::types::{
    Event, Message, Part, PermissionAskedData, Session, SessionStatus, Todo,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ConnState {
    Connecting,
    Connected,
    Disconnected(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TokenTotals {
    pub input: f64,
    pub output: f64,
    pub reasoning: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub cost: f64,
}

impl TokenTotals {
    pub fn add_step(
        &mut self,
        tokens: &opencode_codes::protocol_generated::types::StepFinishPartTokens,
    ) {
        self.input += tokens.input;
        self.output += tokens.output;
        self.reasoning += tokens.reasoning;
        self.cache_read += tokens.cache.read;
        self.cache_write += tokens.cache.write;
    }

    pub fn total_context(&self) -> f64 {
        self.input + self.cache_read + self.cache_write + self.output + self.reasoning
    }
}

/// One message (user or assistant) with its ordered parts.
#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub info: Message,
    pub parts: Vec<Part>,
}

impl MessageEntry {
    pub fn id(&self) -> &str {
        match &self.info {
            Message::User(m) => &m.id,
            Message::Assistant(m) => &m.id,
        }
    }

    pub fn created_ms(&self) -> f64 {
        match &self.info {
            Message::User(m) => m.time.created,
            Message::Assistant(m) => m.time.created as f64,
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self.info, Message::User(_))
    }

    /// Replace or insert a part by id, keeping append-order for new parts.
    pub fn upsert_part(&mut self, part: Part) {
        let pid = crate::state::part_id(&part).to_string();
        if let Some(slot) = self.parts.iter_mut().find(|p| part_id(p) == pid.as_str()) {
            *slot = part;
        } else {
            self.parts.push(part);
        }
    }
}

pub fn part_id(part: &Part) -> &str {
    match part {
        Part::Text(p) => &p.id,
        Part::Subtask(p) => &p.id,
        Part::Reasoning(p) => &p.id,
        Part::File(p) => &p.id,
        Part::Tool(p) => &p.id,
        Part::StepStart(p) => &p.id,
        Part::StepFinish(p) => &p.id,
        Part::Snapshot(p) => &p.id,
        Part::Patch(p) => &p.id,
        Part::Agent(p) => &p.id,
        Part::Retry(p) => &p.id,
        Part::Compaction(p) => &p.id,
    }
}

#[derive(Debug, Clone)]
pub struct PendingPermission {
    pub session_id: String,
    pub permission_id: String,
    pub kind: String,
    pub patterns: Vec<String>,
    pub metadata: serde_json::Map<String, serde_json::Value>,
    pub tool_call_id: Option<String>,
}

#[derive(Clone)]
pub struct Store {
    pub conn: ConnState,
    pub sessions: BTreeMap<String, Session>,
    /// session id → message entries in arrival order (sorted by created time on reconcile)
    pub messages: HashMap<String, Vec<MessageEntry>>,
    pub statuses: HashMap<String, SessionStatus>,
    pub todos: HashMap<String, Vec<Todo>>,
    pub diffs: BTreeMap<String, serde_json::Value>,
    pub providers: Vec<ProviderInfo>,
    pub active_session: Option<String>,
    pub pending_permissions: BTreeMap<String, PendingPermission>,
    pub errors: VecDeque<String>,
    pub totals: TokenTotals,
    pub event_count: u64,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub provider_id: String,
    pub provider_name: String,
    pub models: Vec<(String, String)>, // (model_id, display name)
}

impl Default for Store {
    fn default() -> Self {
        Self {
            conn: ConnState::Connecting,
            sessions: BTreeMap::new(),
            messages: HashMap::new(),
            statuses: HashMap::new(),
            todos: HashMap::new(),
            diffs: BTreeMap::new(),
            providers: Vec::new(),
            active_session: None,
            pending_permissions: BTreeMap::new(),
            errors: VecDeque::new(),
            totals: TokenTotals::default(),
            event_count: 0,
        }
    }
}

impl Store {
    pub fn push_error(&mut self, msg: impl Into<String>) {
        let text = msg.into();
        tracing::warn!("store error: {text}");
        self.errors.push_back(text);
        while self.errors.len() > 50 {
            self.errors.pop_front();
        }
    }

    pub fn active_messages(&self) -> Option<&Vec<MessageEntry>> {
        self.active_session
            .as_ref()
            .and_then(|id| self.messages.get(id))
    }

    pub fn is_busy(&self) -> bool {
        self.active_session
            .as_ref()
            .and_then(|id| self.statuses.get(id))
            .is_some_and(|s| !matches!(s, SessionStatus::Idle))
    }

    /// Apply one decoded SSE event. Returns true if the store changed in a way
    /// that warrants an immediate reconcile for the affected session.
    pub fn apply_event(&mut self, ev: &Event) -> bool {
        use opencode_codes::protocol_generated::types::Event::*;
        self.event_count += 1;
        match ev {
            SessionCreated(e) => {
                self.sessions
                    .insert(e.properties.info.id.clone(), e.properties.info.clone());
                false
            }
            SessionUpdated(e) => {
                self.sessions
                    .insert(e.properties.info.id.clone(), e.properties.info.clone());
                false
            }
            SessionDeleted(e) => {
                let sid = e.properties.info.id.clone();
                self.sessions.remove(&sid);
                self.messages.remove(&sid);
                self.statuses.remove(&sid);
                if self.active_session.as_deref() == Some(sid.as_str()) {
                    self.active_session = None;
                }
                false
            }
            SessionStatus(e) => {
                let p = &e.properties;
                self.statuses.insert(p.session_id.clone(), p.status.clone());
                false
            }
            SessionIdle(e) => {
                self.statuses.insert(
                    e.properties.session_id.clone(),
                    opencode_codes::protocol_generated::types::SessionStatus::Idle,
                );
                true // final state may have trailing updates; reconcile once more
            }
            MessageUpdated(e) => {
                self.ensure_message(e.properties.session_id.clone(), e.properties.info.clone());
                false
            }
            MessageRemoved(e) => {
                if let Some(list) = self.messages.get_mut(e.properties.session_id.as_str()) {
                    list.retain(|m| m.id() != e.properties.message_id);
                }
                false
            }
            MessagePartUpdated(e) => {
                let data = &e.properties;
                let part = data.part.clone();
                let mid = message_id_of(&part).to_string();
                let entry = self
                    .messages
                    .entry(data.session_id.clone())
                    .or_default()
                    .iter_mut()
                    .find(|m| m.id() == mid);
                match entry {
                    Some(m) => m.upsert_part(part),
                    None => {
                        // Part arrived before its message envelope; synthesize a
                        // placeholder so the timeline shows something.
                        let list = self.messages.entry(data.session_id.clone()).or_default();
                        list.push(MessageEntry {
                            info: placeholder_message(&mid),
                            parts: vec![part],
                        });
                    }
                }
                false
            }
            MessagePartRemoved(e) => {
                let d = &e.properties;
                if let Some(list) = self.messages.get_mut(d.session_id.as_str())
                    && let Some(m) = list.iter_mut().find(|m| m.id() == d.message_id)
                {
                    m.parts.retain(|p| part_id(p) != d.part_id);
                }
                false
            }
            PermissionAsked(e) => {
                let d: &PermissionAskedData = &e.properties;
                self.pending_permissions.insert(
                    d.id.clone(),
                    PendingPermission {
                        session_id: d.session_id.clone(),
                        permission_id: d.id.clone(),
                        kind: d.permission.clone(),
                        patterns: d.patterns.clone(),
                        metadata: d.metadata.clone(),
                        tool_call_id: d.tool.as_ref().map(|t| t.call_id.clone()),
                    },
                );
                false
            }
            PermissionReplied(_) | PermissionV2Replied(_) => {
                false // reply clears handled after successful REST call
            }
            TodoUpdated(e) => {
                self.todos
                    .insert(e.properties.session_id.clone(), e.properties.todos.clone());
                false
            }
            SessionDiff(e) => {
                self.diffs.insert(
                    e.properties.session_id.clone(),
                    serde_json::to_value(&e.properties).unwrap_or(serde_json::Value::Null),
                );
                false
            }
            SessionError(e) => {
                if let Some(err) = &e.properties.error {
                    self.push_error(format!("session error: {}", error_brief(err)));
                }
                false
            }
            _ => false,
        }
    }

    /// Insert or replace a message envelope, keeping entries sorted by creation time.
    pub fn ensure_message(&mut self, session_id: String, info: Message) {
        let list = self.messages.entry(session_id).or_default();
        if let Some(existing) = list
            .iter_mut()
            .find(|m| m.id() == message_id_of_info(&info))
        {
            existing.info = info;
        } else {
            let entry = MessageEntry {
                info,
                parts: Vec::new(),
            };
            let ts = entry.created_ms();
            let pos = list.partition_point(|m| m.created_ms() <= ts);
            list.insert(pos, entry);
        }
    }

    /// Replace the full authoritative message list for a session.
    pub fn set_messages(&mut self, session_id: &str, mut msgs: Vec<MessageEntry>) {
        msgs.sort_by(|a, b| a.created_ms().total_cmp(&b.created_ms()));
        self.messages.insert(session_id.to_string(), msgs);
    }

    /// Recompute cumulative token totals from all assistant messages of the active session.
    pub fn recompute_totals(&mut self) {
        let Some(active) = self.active_session.clone() else {
            return;
        };
        let mut t = TokenTotals::default();
        if let Some(list) = self.messages.get(&active) {
            for m in list {
                if let Message::Assistant(a) = &m.info {
                    t.add_step(&a.tokens);
                    t.cost += a.cost;
                }
            }
        }
        self.totals = t;
    }
}

fn message_id_of(part: &Part) -> &str {
    match part {
        Part::Text(p) => &p.message_id,
        Part::Subtask(p) => &p.message_id,
        Part::Reasoning(p) => &p.message_id,
        Part::File(p) => &p.message_id,
        Part::Tool(p) => &p.message_id,
        Part::StepStart(p) => &p.message_id,
        Part::StepFinish(p) => &p.message_id,
        Part::Snapshot(p) => &p.message_id,
        Part::Patch(p) => &p.message_id,
        Part::Agent(p) => &p.message_id,
        Part::Retry(p) => &p.message_id,
        Part::Compaction(p) => &p.message_id,
    }
}

fn message_id_of_info(info: &Message) -> &str {
    match info {
        Message::User(m) => &m.id,
        Message::Assistant(m) => &m.id,
    }
}

/// A minimal user-message shell used when a part beats its envelope.
fn placeholder_message(id: &str) -> Message {
    serde_json::from_value(serde_json::json!({
        "type": "user",
        "id": id,
        "agent": "ade",
        "model": { "providerID": "?", "modelID": "?" },
        "role": "user",
        "time": { "created": 0.0 },
        "sessionID": ""
    }))
    .expect("placeholder user message shape")
}

fn error_brief(err: &opencode_codes::protocol_generated::types::SessionErrorDataError) -> String {
    use opencode_codes::protocol_generated::types::SessionErrorDataError as E;
    match err {
        E::ProviderAuthError(e) => format!("provider auth: {}", e.data.message),
        E::UnknownError(e) => e.data.message.clone(),
        E::MessageOutputLengthError(_) => "output length exceeded".into(),
        E::MessageAbortedError(_) => "aborted".into(),
        E::StructuredOutputError(e) => format!("structured output: {}", e.data.message),
        E::ContextOverflowError(e) => format!("context overflow: {}", e.data.message),
        E::ContentFilterError(e) => format!("content filter: {}", e.data.message),
        E::APIError(e) => format!("api [{}] {}", e.name, e.data.message),
    }
}
