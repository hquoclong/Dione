//! Context-window compiler tests.

use ade_core::context::{SectionKind, compile};
use ade_core::state::Store;
use opencode_codes::protocol_generated::types::Event;

fn ev(json: &str) -> Event {
    serde_json::from_str(json).expect("valid test event")
}

#[test]
fn empty_store_yields_default_view() {
    let view = compile(&Store::default());
    assert!(view.sections.is_empty());
    assert_eq!(view.est_total_tokens, 0);
}

#[test]
fn sections_follow_wire_order_with_usage_anchor() {
    let mut store = Store::default();
    store.apply_event(&ev(r#"{
        "type": "message.updated",
        "id": "e1",
        "properties": { "info": {
            "type": "user", "id": "mu", "agent": "build",
            "model": { "providerID": "p", "modelID": "m" },
            "role": "user", "sessionID": "s",
            "time": { "created": 1.0 } }, "sessionID": "s" }
    }"#));
    store.apply_event(&ev(r#"{
        "type": "message.part.updated", "id": "e2",
        "properties": { "part": {
            "type": "text", "id": "pu", "messageID": "mu",
            "sessionID": "s", "text": "hello there agent" },
            "sessionID": "s", "time": 1.0 }
    }"#));
    store.active_session = Some("s".into());

    // Assistant message carries real usage numbers. Reconcile semantics:
    // set_messages replaces the session's list wholesale, so carry over the
    // user entry built by the SSE events above.
    let user_entry = store.messages.get("s").unwrap()[0].clone();
    let mwp: opencode_codes::protocol_generated::types::MessageWithParts =
        serde_json::from_str(
            r#"{
            "info": {
                "type": "assistant", "id": "ma", "agent": "build", "cost": 0.02,
                "mode": "build", "modelID": "m", "parentID": "",
                "path": {"cwd":"/","root":"/"}, "providerID": "p",
                "role": "assistant", "sessionID": "s",
                "time": {"created": 2},
                "tokens": {"cache":{"read":500,"write":0},"input":100,"output":40,"reasoning":10}
            },
            "parts": [
                { "type": "reasoning", "id": "pr", "messageID": "ma", "sessionID": "s",
                  "text": "pondering", "time": {"start": 1, "end": 2} },
                { "type": "tool", "id": "pt", "callID": "c1", "messageID": "ma",
                  "sessionID": "s", "tool": "read",
                  "state": { "status": "completed", "input": {"path":"a.rs"},
                             "metadata": {}, "output": "contents",
                             "title": "a.rs", "time": {"start": 1, "end": 2} } },
                { "type": "text", "id": "pa", "messageID": "ma", "sessionID": "s",
                  "text": "here is the answer" },
                { "type": "step-finish", "id": "pf", "messageID": "ma", "sessionID": "s",
                  "cost": 0.02, "reason": "stop",
                  "tokens": {"cache":{"read":500,"write":0},"input":100,"output":40,"reasoning":10} }
            ]
        }"#,
        )
        .unwrap();
    store.set_messages(
        "s",
        vec![
            user_entry,
            ade_core::MessageEntry {
                info: mwp.info,
                parts: mwp.parts,
            },
        ],
    );

    let view = compile(&store);

    let labels: Vec<&str> = view.sections.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels[0], "system prompt");
    assert!(labels.iter().any(|l| l.starts_with("user ·")));
    assert!(labels.contains(&"reasoning"));
    assert!(
        labels
            .iter()
            .any(|l| l.starts_with("tool:read [completed]"))
    );
    assert!(labels.iter().any(|l| l.starts_with("assistant text")));

    // Usage anchors come from the newest assistant message.
    assert_eq!(view.actual_input_tokens, Some(100.0));
    assert_eq!(view.actual_cache_read, Some(500.0));
    assert_eq!(view.actual_output_tokens, Some(40.0));

    assert!(view.est_total_tokens > 0);
    assert!(
        matches!(view.sections[1].kind, SectionKind::User),
        "wire order preserved"
    );
}
