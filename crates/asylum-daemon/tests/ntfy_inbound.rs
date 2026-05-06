/// Integration test: ntfy JSON-stream subscriber
///
/// Spins up a tiny axum test server, configures CapabilityService to point at
/// it, starts background tasks, then waits for the inbound message to appear
/// in channel_messages and the channel.inbound hook event to fire.
use std::sync::Arc;
use std::time::Duration;

use asylum_daemon::auth::AuthMode;
use asylum_daemon::capability_service::{AppConfig, CapabilityService};
use asylum_daemon::storage::Store;
use asylum_types::config::AsylumConfig;
use asylum_types::node::CapabilitySnapshot;
use asylum_types::node::{HarnessKind, SubstrateKind};
use axum::body::Body;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde_json::json;
use time::OffsetDateTime;
use tokio::net::TcpListener;

fn make_config(ntfy_server: String, ntfy_topic: String) -> AppConfig {
    let core = AsylumConfig::default();
    AppConfig {
        base_url: core.base_url,
        bind_addr: "127.0.0.1:0".to_string(),
        socket_path: None,
        transcripts_dir: "/tmp/asylum-test-ntfy/transcripts".to_string(),
        workspace_recent_limit: 20,
        ntfy_server: Some(ntfy_server),
        ntfy_topic: Some(ntfy_topic),
        ntfy_token: None,
        ntfy_poll_interval_seconds: Some(2),
        harness: core.harness,
        loon: core.loon,
    }
}

/// Handler that returns two NDJSON lines: an open event then a message event.
async fn ntfy_stream_handler(Path(topic): Path<String>) -> Response<Body> {
    let line1 =
        format!("{{\"id\":\"a\",\"time\":1000,\"event\":\"open\",\"topic\":\"{topic}\"}}\n");
    let line2 = format!(
        "{{\"id\":\"b\",\"time\":1001,\"event\":\"message\",\"topic\":\"{topic}\",\"message\":\"hello\",\"title\":\"approve\",\"tags\":[]}}\n"
    );
    // Concatenate both lines; reqwest will read them as a stream.
    let body_bytes = format!("{line1}{line2}");
    Response::builder()
        .status(200)
        .header("content-type", "application/x-ndjson")
        .body(Body::from(body_bytes))
        .unwrap()
}

#[tokio::test]
async fn ntfy_inbound_message_is_recorded() {
    // Start mock ntfy server on an OS-assigned port.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let app = Router::new().route("/{topic}/json", get(ntfy_stream_handler));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let server_url = format!("http://{addr}");
    let topic = "asylum-test".to_string();

    // Build an in-memory store and CapabilityService.
    let store = Store::open_in_memory().expect("open in-memory store");
    let config = make_config(server_url, topic);
    let service = Arc::new(CapabilityService::new(
        store.clone(),
        AuthMode::Disabled,
        config,
    ));

    // Subscribe to hook events before starting background tasks.
    let mut hook_rx = service.hook_engine.subscribe();

    // Start background tasks — this spawns the ntfy subscriber.
    service.start_background_tasks();

    // Wait up to 5 seconds for the inbound message row to appear.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut found_row = false;
    while tokio::time::Instant::now() < deadline {
        let messages = store
            .list_channel_messages("ntfy-default", 10)
            .expect("list messages");
        if messages
            .iter()
            .any(|m| m.direction == "in" && m.body == "hello")
        {
            found_row = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        found_row,
        "expected an inbound message with body='hello' in channel_messages within 5s"
    );

    // Also check that the channel.inbound hook event was posted.
    // Drain the broadcast receiver for up to 1s looking for the event.
    let event_deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    let mut found_event = false;
    loop {
        match tokio::time::timeout_at(event_deadline, hook_rx.recv()).await {
            Ok(Ok(event)) if event.event == "channel.inbound" => {
                found_event = true;
                break;
            }
            Ok(Ok(_)) => continue, // other events
            _ => break,            // timeout or closed
        }
    }
    assert!(
        found_event,
        "expected a channel.inbound hook event to be posted"
    );
}

#[tokio::test]
async fn ntfy_inbound_correlated_message_does_not_record_when_routing_fails() {
    // Start mock ntfy server on an OS-assigned port.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let marked = "must not persist\n\n[asylum-reply:abcde]".to_string();
    let app = Router::new().route(
        "/{topic}/json",
        get(move |_path: Path<String>| async move {
            let line1 =
                format!("{{\"id\":\"a\",\"time\":1000,\"event\":\"open\",\"topic\":\"topic\"}}\n");
            let line2 = format!(
                "{}\n",
                json!({
                    "id": "b",
                    "time": 1001,
                    "event": "message",
                    "topic": "topic",
                    "message": marked,
                    "title": "route",
                    "tags": [],
                })
            );
            let body_bytes = format!("{line1}{line2}");
            Response::builder()
                .status(200)
                .header("content-type", "application/x-ndjson")
                .body(Body::from(body_bytes))
                .unwrap()
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let server_url = format!("http://{addr}");
    let topic = "asylum-test".to_string();

    let store = Store::open_in_memory().expect("open in-memory store");
    let node = store
        .insert_node(
            HarnessKind::Codex,
            SubstrateKind::Local,
            "worker",
            Some("/tmp"),
            Some("reply-test"),
            None,
            CapabilitySnapshot::default(),
            None,
        )
        .expect("insert node");
    let config = make_config(server_url, topic);
    let service = Arc::new(CapabilityService::new(
        store.clone(),
        AuthMode::Disabled,
        config,
    ));
    store
        .insert_channel_reply_correlation(
            "abcde",
            "ntfy-default",
            node.id,
            OffsetDateTime::now_utc().unix_timestamp() + 60,
        )
        .expect("insert correlation");

    service.start_background_tasks();

    tokio::time::sleep(Duration::from_millis(250)).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if store
            .list_channel_messages("ntfy-default", 10)
            .expect("list messages")
            .into_iter()
            .any(|message| message.body.contains("must not persist"))
        {
            panic!("failed: correlated inbound message was recorded despite routing failure");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
