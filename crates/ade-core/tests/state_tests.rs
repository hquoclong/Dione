//! Unit tests for the state mirror: SSE event application, ordering,
//! reconciliation, and totals.

use ade_core::state::Store;
use opencode_codes::protocol_generated::types::Event;

fn ev(json: &str) -> Event {
    serde_json::from_str(json).expect("valid test event")
}

const SESSION: &str = r#"{
    "type": "session.created",
    "id": "evt_s1",
    "properties": {
        "info": {
            "id": "ses_1",
            "title": "test",
            "directory": "/tmp",
            "projectID": "proj_1",
            "slug": "test",
            "version": "1.0",
            "time": { "created": 1, "updated": 1 }
        },
        "sessionID": "ses_1"
    }
}"#;

#[test]
fn session_created_registers_session() {
    let mut store = Store::default();
    assert!(store.sessions.is_empty());
    store.apply_event(&ev(SESSION));
    assert!(store.sessions.contains_key("ses_1"));
}

#[test]
fn message_updated_then_part_updated_assembles_timeline() {
    let mut store = Store::default();
    store.apply_event(&ev(SESSION));

    // User envelope arrives first…
    store.apply_event(&ev(r#"{
        "type": "message.updated",
        "id": "evt_m1",
        "properties": {
            "info": {
                "type": "user",
                "id": "msg_u1",
                "agent": "build",
                "model": { "providerID": "anthropic", "modelID": "claude-x" },
                "role": "user",
                "sessionID": "ses_1",
                "time": { "created": 10.0 }
            },
            "sessionID": "ses_1"
        }
    }"#));

    // …then its text part streams in.
    let changed = store.apply_event(&ev(r#"{
        "type": "message.part.updated",
        "id": "evt_p1",
        "properties": {
            "part": {
                "type": "text",
                "id": "prt_1",
                "messageID": "msg_u1",
                "sessionID": "ses_1",
                "text": "hello world"
            },
            "sessionID": "ses_1",
            "time": 11.0
        }
    }"#));
    assert!(!changed);

    let msgs = store.messages.get("ses_1").unwrap();
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].is_user());
    assert_eq!(msgs[0].parts.len(), 1);
}

#[test]
fn part_before_envelope_creates_placeholder() {
    let mut store = Store::default();
    store.apply_event(&ev(SESSION));

    store.apply_event(&ev(r#"{
        "type": "message.part.updated",
        "id": "evt_p1",
        "properties": {
            "part": {
                "type": "tool",
                "id": "prt_t1",
                "callID": "call_1",
                "messageID": "msg_a9",
                "sessionID": "ses_1",
                "tool": "bash",
                "state": { "status": "pending", "input": {"command":"ls"}, "raw": "" }
            },
            "sessionID": "ses_1",
            "time": 5.0
        }
    }"#));

    let msgs = store.messages.get("ses_1").unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].id(), "msg_a9");
}

#[test]
fn upsert_replaces_part_by_id_not_append() {
    let mut store = Store::default();
    store.apply_event(&ev(SESSION));

    for text in ["first", "second"] {
        store.apply_event(&ev(&format!(
            r#"{{
            "type": "message.part.updated",
            "id": "evt_x",
            "properties": {{
                "part": {{
                    "type": "text",
                    "id": "prt_same",
                    "messageID": "msg_u1",
                    "sessionID": "ses_1",
                    "text": "{text}"
                }},
                "sessionID": "ses_1",
                "time": 5.0
            }}
        }}"#,
        )));
    }

    let msgs = store.messages.get("ses_1").unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].parts.len(), 1);
}

#[test]
fn idle_sets_status_and_requests_reconcile() {
    let mut store = Store::default();
    store.apply_event(&ev(SESSION));
    store.active_session = Some("ses_1".into());
    store.apply_event(&ev(
        r#"{"type":"session.status","id":"e","properties":{"sessionID":"ses_1","status":{"type":"busy"}}}"#,
    ));
    assert!(store.is_busy());

    let changed = store.apply_event(&ev(
        r#"{"type":"session.idle","id":"e","properties":{"sessionID":"ses_1"}}"#,
    ));
    assert!(changed, "idle should hint a final reconcile");
    assert!(!store.is_busy());
}

#[test]
fn permission_asked_populates_gate() {
    let mut store = Store::default();
    store.apply_event(&ev(r#"{
        "type": "permission.asked",
        "id": "evt_per",
        "properties": {
            "always": [],
            "id": "per_1",
            "metadata": { "command": "rm -rf /" },
            "patterns": ["**"],
            "permission": "bash",
            "sessionID": "ses_1"
        }
    }"#));

    let pending = store.pending_permissions.get("per_1").unwrap();
    assert_eq!(pending.kind, "bash");
    assert_eq!(pending.session_id, "ses_1");
}

#[test]
fn set_messages_sorts_by_time_and_totals_accumulate() {
    let mut store = Store {
        active_session: Some("ses_1".into()),
        ..Default::default()
    };

    let assistant = |created: u64, input: f64| -> String {
        format!(
            r#"{{
            "type": "assistant",
            "id": "msg_{created}",
            "agent": "build",
            "cost": 0.01,
            "mode": "build",
            "modelID": "m",
            "parentID": "",
            "path": {{ "cwd": "/", "root": "/" }},
            "providerID": "anthropic",
            "role": "assistant",
            "sessionID": "ses_1",
            "time": {{ "created": {created} }},
            "tokens": {{
                "cache": {{ "read": 0, "write": 0 }},
                "input": {input},
                "output": 7,
                "reasoning": 0
            }}
        }}"#
        )
    };

    // Deliberately out of order.
    let raw = format!(
        "[{{\"info\":{},\"parts\":[]}},{{\"info\":{},\"parts\":[]}}]",
        assistant(200, 20.0),
        assistant(100, 10.0)
    );
    let parsed: Vec<opencode_codes::protocol_generated::types::MessageWithParts> =
        serde_json::from_str(&raw).unwrap();

    store.set_messages(
        "ses_1",
        parsed
            .into_iter()
            .map(|m| ade_core::MessageEntry {
                info: m.info,
                parts: m.parts,
            })
            .collect(),
    );
    store.recompute_totals();

    let msgs = store.active_messages().unwrap();
    assert_eq!(msgs[0].created_ms(), 100.0);
    assert_eq!(msgs[1].created_ms(), 200.0);
    assert!((store.totals.input - 30.0).abs() < f64::EPSILON);
    assert!((store.totals.output - 14.0).abs() < f64::EPSILON);
    assert!((store.totals.cost - 0.02).abs() < 1e-9);
}

#[test]
fn error_ring_buffer_caps_at_50() {
    let mut store = Store::default();
    for i in 0..60 {
        store.push_error(format!("err-{i}"));
    }
    assert_eq!(store.errors.len(), 50);
    assert_eq!(store.errors.front().unwrap(), "err-10");
}
