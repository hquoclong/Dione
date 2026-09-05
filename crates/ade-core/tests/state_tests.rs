//! State-machine tests driven by real opencode wire JSON.
//! Fast, deterministic, no network.

use ade_core::Store;
use opencode_codes::protocol_generated::types::{Event, SessionStatus};
use serde_json::json;

fn session_json(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "directory": "/repo",
        "projectID": "proj_1",
        "slug": "test",
        "time": {"created": 1, "updated": 2},
        "title": "test session",
        "version": "1"
    })
}

fn apply(store: &mut Store, v: serde_json::Value) {
    let ev: Event = serde_json::from_value(v).expect("event must decode");
    store.apply_event(&ev);
}

#[test]
fn session_created_becomes_active() {
    let mut s = Store::default();
    apply(
        &mut s,
        json!({
            "type": "session.created",
            "id": "evt_1",
            "properties": {"info": session_json("ses_1"), "sessionID": "ses_1"}
        }),
    );
    assert_eq!(s.active_session.as_deref(), Some("ses_1"));
    assert_eq!(s.sessions["ses_1"].title, "test session");
}

#[test]
fn status_busy_marks_busy() {
    let mut s = Store {
        active_session: Some("ses_1".into()),
        ..Default::default()
    };
    apply(
        &mut s,
        json!({
            "type": "session.status",
            "id": "evt_2",
            "properties": {"sessionID": "ses_1", "status": {"type": "busy"}}
        }),
    );
    assert!(matches!(s.statuses["ses_1"], SessionStatus::Busy));
    assert!(s.is_busy());
}

#[test]
fn session_idle_clears_busy() {
    let mut s = Store {
        active_session: Some("ses_1".into()),
        ..Default::default()
    };
    s.statuses.insert("ses_1".into(), SessionStatus::Busy);
    apply(
        &mut s,
        json!({"type": "session.idle", "id": "evt_3", "properties": {"sessionID": "ses_1"}}),
    );
    assert!(matches!(s.statuses["ses_1"], SessionStatus::Idle));
    assert!(!s.is_busy());
}

#[test]
fn permission_asked_collects_pending() {
    let mut s = Store::default();
    apply(
        &mut s,
        json!({
            "type": "permission.asked",
            "id": "evt_4",
            "properties": {
                "id": "per_1",
                "sessionID": "ses_1",
                "permission": "bash",
                "patterns": ["cargo *"],
                "metadata": {"command": "cargo test"},
                "always": []
            }
        }),
    );
    let p = &s.pending_permissions["per_1"];
    assert_eq!(p.session_id, "ses_1");
    assert_eq!(p.kind, "bash");
    assert_eq!(p.patterns, vec!["cargo *".to_string()]);
}

#[test]
fn permission_replied_clears_pending() {
    let mut s = Store::default();
    apply(
        &mut s,
        json!({
            "type": "permission.asked",
            "id": "evt_4",
            "properties": {
                "id": "per_1",
                "sessionID": "ses_1",
                "permission": "bash",
                "patterns": [],
                "metadata": {},
                "always": []
            }
        }),
    );
    apply(
        &mut s,
        json!({
            "type": "permission.replied",
            "id": "evt_5",
            "properties": {"requestID": "per_1", "sessionID": "ses_1", "reply": "once"}
        }),
    );
    assert!(s.pending_permissions.is_empty());
}

#[test]
fn message_updated_upserts_info() {
    let mut s = Store::default();
    let user = json!({
        "role": "user",
        "id": "msg_1",
        "agent": "opencode",
        "model": {"modelID": "m", "providerID": "p"},
        "sessionID": "ses_1",
        "time": {"created": 1.0}
    });
    apply(
        &mut s,
        json!({
            "type": "message.updated",
            "id": "evt_6",
            "properties": {"info": user, "sessionID": "ses_1"}
        }),
    );
    assert_eq!(s.messages["ses_1"].len(), 1);
    // Second update for the same id replaces instead of duplicating.
    apply(
        &mut s,
        json!({
            "type": "message.updated",
            "id": "evt_7",
            "properties": {"info": user, "sessionID": "ses_1"}
        }),
    );
    assert_eq!(s.messages["ses_1"].len(), 1);
}

#[test]
fn todo_updated_stores_todos() {
    let mut s = Store::default();
    apply(
        &mut s,
        json!({
            "type": "todo.updated",
            "id": "evt_8",
            "properties": {
                "sessionID": "ses_1",
                "todos": [{"content": "write tests", "priority": "high", "status": "in_progress"}]
            }
        }),
    );
    assert_eq!(s.todos["ses_1"][0].content, "write tests");
}

#[test]
fn session_deleted_falls_back_active() {
    let mut s = Store::default();
    for id in ["ses_1", "ses_2"] {
        apply(
            &mut s,
            json!({
                "type": "session.created",
                "id": format!("evt_{id}"),
                "properties": {"info": session_json(id), "sessionID": id}
            }),
        );
    }
    assert_eq!(s.active_session.as_deref(), Some("ses_1"));
    apply(
        &mut s,
        json!({
            "type": "session.deleted",
            "id": "evt_del",
            "properties": {"info": session_json("ses_1"), "sessionID": "ses_1"}
        }),
    );
    assert!(!s.sessions.contains_key("ses_1"));
    assert_eq!(s.active_session.as_deref(), Some("ses_2"));
}

#[test]
fn retired_session_frames_are_dropped() {
    use ade_core::state::event_session_id;

    let mut s = Store::default();
    apply(
        &mut s,
        json!({
            "type": "session.created",
            "id": "evt_1",
            "properties": {"info": session_json("ses_1"), "sessionID": "ses_1"}
        }),
    );
    s.session_scope.insert("ses_1".into(), "feat-a".into());
    s.retire_session("ses_1");
    assert!(!s.sessions.contains_key("ses_1"));

    // A late status frame must not resurrect anything.
    apply(
        &mut s,
        json!({
            "type": "session.status",
            "id": "evt_2",
            "properties": {"sessionID": "ses_1", "status": {"type": "busy"}}
        }),
    );
    assert!(!s.statuses.contains_key("ses_1"));

    // event_session_id covers the variants the pumps route on.
    let ev: Event = serde_json::from_value(json!({
        "type": "permission.asked",
        "id": "evt_3",
        "properties": {
            "id": "per_9", "sessionID": "ses_9", "permission": "bash",
            "patterns": [], "metadata": {}, "always": []
        }
    }))
    .unwrap();
    assert_eq!(event_session_id(&ev), Some("ses_9"));
}

#[test]
fn worktree_status_derives_from_session() {
    use ade_core::WorktreeStatus;
    use std::path::Path;

    let mut s = Store::default();
    let mut r = ade_core::worktree::WorktreeRecord::new(Path::new("/repo"), "feat-a").unwrap();
    // No session yet -> Creating.
    s.worktrees.insert(r.slug.clone(), r.clone());
    assert_eq!(s.worktree_status("feat-a"), WorktreeStatus::Creating);

    // Link a session: no messages, idle -> still Creating.
    r.session_id = Some("ses_1".into());
    s.worktrees.insert(r.slug.clone(), r);
    s.session_scope.insert("ses_1".into(), "feat-a".into());
    apply(
        &mut s,
        json!({
            "type": "session.created",
            "id": "evt_1",
            "properties": {"info": session_json("ses_1"), "sessionID": "ses_1"}
        }),
    );
    assert_eq!(s.worktree_status("feat-a"), WorktreeStatus::Creating);

    // Busy -> Working.
    s.statuses.insert("ses_1".into(), SessionStatus::Busy);
    assert_eq!(s.worktree_status("feat-a"), WorktreeStatus::Working);

    // Pending permission beats busy -> NeedsYou.
    apply(
        &mut s,
        json!({
            "type": "permission.asked",
            "id": "evt_2",
            "properties": {
                "id": "per_1", "sessionID": "ses_1", "permission": "bash",
                "patterns": [], "metadata": {}, "always": []
            }
        }),
    );
    assert_eq!(s.worktree_status("feat-a"), WorktreeStatus::NeedsYou);
}

#[test]
fn sessions_group_by_scope() {
    let mut s = Store::default();
    for id in ["ses_1", "ses_2", "ses_3"] {
        apply(
            &mut s,
            json!({
                "type": "session.created",
                "id": format!("evt_{id}"),
                "properties": {"info": session_json(id), "sessionID": id}
            }),
        );
    }
    s.session_scope.insert("ses_1".into(), "feat-a".into());
    s.session_scope.insert("ses_2".into(), "feat-a".into());
    // ses_3 stays in root scope.
    assert_eq!(s.sessions_in_scope("feat-a").len(), 2);
    assert_eq!(s.sessions_in_scope(""), vec!["ses_3".to_string()]);
    assert_eq!(s.scope_of("ses_9"), "");
}
