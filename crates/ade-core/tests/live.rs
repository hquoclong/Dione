//! Live integration tests against a real `opencode serve`.
//!
//! Run with:
//!   cargo test -p ade-core --features integration-tests
//!
//! Tier A (always runs with the feature): server lifecycle, sessions, SSE.
//! Tier B (set ADE_LIVE_PROMPT=1): a full prompt→response round trip; needs a
//! working LLM provider.

#![cfg(feature = "integration-tests")]

use std::time::Duration;

use ade_core::{AdeServer, AppConfig};
use opencode_codes::client_async::OpencodeClient;
use opencode_codes::sse::{RetryConfig, StreamEvent};

fn test_config() -> AppConfig {
    let dir = std::env::temp_dir().join(format!("ade-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    AppConfig {
        project_dir: dir,
        ..AppConfig::default()
    }
}

async fn spawn_server() -> (AdeServer, OpencodeClient) {
    let server = AdeServer::start(&test_config())
        .await
        .expect("server starts");
    let client = server.client.clone();
    assert!(server.health().await.expect("health"), "server healthy");
    (server, client)
}

#[tokio::test]
async fn tier_a_server_sessions_and_sse() {
    let (mut server, client) = spawn_server().await;

    // Session create → list → messages(empty) → abort → delete.
    let session = client
        .create_session(
            &opencode_codes::protocol_generated::types::SessionCreateParams {
                title: Some("ade tier-a probe".into()),
                ..Default::default()
            },
        )
        .await
        .expect("create session");
    assert!(session.id.starts_with("ses"));

    let empty = client.list_messages(&session.id).await.unwrap();
    assert!(empty.is_empty());

    let aborted = client.abort(&session.id).await.unwrap();
    let _ = aborted;

    client
        .request_unit(
            reqwest::Method::DELETE,
            &format!("/session/{}", session.id),
            None,
        )
        .await
        .expect("delete session");

    // SSE stream opens and delivers the connected marker.
    let mut events = client.event_stream(RetryConfig::default()).unwrap();
    let first = tokio::time::timeout(Duration::from_secs(15), events.next())
        .await
        .expect("SSE open timed out")
        .expect("stream ended")
        .expect("stream error");
    assert!(
        first.is_connected(),
        "expected Connected as first item, got {first:?}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn tier_b_full_prompt_roundtrip() {
    if std::env::var("ADE_LIVE_PROMPT").is_err() {
        eprintln!("skipping tier B (set ADE_LIVE_PROMPT=1 to enable)");
        return;
    }

    let (mut server, client) = spawn_server().await;
    let session = client
        .create_session(
            &opencode_codes::protocol_generated::types::SessionCreateParams {
                title: Some("ade tier-b prompt".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let mut events = client.event_stream(RetryConfig::default()).unwrap();

    use opencode_codes::protocol_generated::types::{
        PromptAsyncParams, PromptAsyncParamsPartsItem, TextPartInput,
    };
    client
        .prompt_async(
            &session.id,
            &PromptAsyncParams {
                parts: vec![PromptAsyncParamsPartsItem::Text(TextPartInput {
                    text: "Reply with exactly: OK".into(),
                    ..Default::default()
                })],
                ..Default::default()
            },
        )
        .await
        .expect("prompt accepted");

    // Pump until idle, reconciling periodically.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("timed out waiting for idle");
        }
        let item = tokio::time::timeout(Duration::from_secs(10), events.next())
            .await
            .expect("event wait timed out");
        match item {
            None => panic!("stream closed"),
            Some(Ok(StreamEvent::Connected)) => {}
            Some(Ok(StreamEvent::Event(ev))) => {
                if matches!(
                    ev.as_ref(),
                    opencode_codes::protocol_generated::types::Event::SessionIdle(_)
                ) {
                    break;
                }
            }
            Some(Ok(StreamEvent::Unknown(_))) => {}
            Some(Ok(_)) => {} // StreamEvent is non_exhaustive
            Some(Err(e)) => panic!("stream error: {e}"),
        }
    }

    let msgs = client.list_messages(&session.id).await.unwrap();
    assert!(!msgs.is_empty(), "messages exist after turn");
    let has_assistant_text = msgs.iter().any(|m| {
        m.parts.iter().any(|p| {
            matches!(p, opencode_codes::protocol_generated::types::Part::Text(t) if !t.text.is_empty())
        })
    });
    assert!(has_assistant_text, "assistant produced text");

    client
        .request_unit(
            reqwest::Method::DELETE,
            &format!("/session/{}", session.id),
            None,
        )
        .await
        .ok();
    server.shutdown().await;
}
