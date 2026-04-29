use anyhow::{anyhow, Result};
use futures::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use crate::capability_service::CapabilityService;

#[derive(Debug, Deserialize)]
struct NtfyMessage {
    #[allow(dead_code)]
    #[serde(default)]
    id: String,
    #[allow(dead_code)]
    #[serde(default)]
    time: u64,
    event: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    tags: Vec<String>,
    #[allow(dead_code)]
    #[serde(default)]
    topic: String,
}

pub struct NtfyInboundConfig {
    pub server: String,
    pub topic: String,
    pub channel_id: String,
    /// Floor for reconnect backoff in seconds. Doubled on each consecutive failure up to 60s.
    pub poll_interval_seconds: u64,
    /// Optional Bearer token for authenticated ntfy topics.
    pub token: Option<String>,
}

/// Runs the ntfy JSON-stream subscriber with exponential backoff reconnect.
/// This function never returns in normal operation.
pub async fn run(service: Arc<CapabilityService>, cfg: NtfyInboundConfig) {
    let floor = cfg.poll_interval_seconds.max(2);
    let mut backoff = Duration::from_secs(floor);
    loop {
        match run_subscription(&service, &cfg).await {
            Ok(()) => {
                // EOF from server — reconnect after backoff floor (server closed connection).
                tracing::debug!(
                    target: "ntfy_inbound",
                    topic = %cfg.topic,
                    "ntfy stream closed by server; reconnecting after {}s",
                    backoff.as_secs()
                );
                sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
            Err(error) => {
                tracing::warn!(
                    target: "ntfy_inbound",
                    topic = %cfg.topic,
                    "subscription error: {error}; reconnecting after {}s",
                    backoff.as_secs()
                );
                sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        }
    }
}

async fn run_subscription(service: &Arc<CapabilityService>, cfg: &NtfyInboundConfig) -> Result<()> {
    let url = format!(
        "{}/{}/json",
        cfg.server.trim_end_matches('/'),
        cfg.topic
    );

    let client = reqwest::Client::builder()
        .pool_idle_timeout(Duration::from_secs(0))
        .build()?;

    let mut req = client.get(&url);
    if let Some(token) = &cfg.token {
        req = req.bearer_auth(token);
    }

    let response = req.send().await?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "ntfy stream returned status {}",
            response.status()
        ));
    }

    // Reset backoff on successful connection is signalled by returning Ok(())
    // with no error; the outer loop handles reconnect.
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::<u8>::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.extend_from_slice(&chunk);

        // Drain complete newline-delimited lines from the buffer.
        while let Some(idx) = buffer.iter().position(|b| *b == b'\n') {
            let line_bytes = buffer.drain(..=idx).collect::<Vec<_>>();
            // Exclude the trailing newline.
            let line_str = std::str::from_utf8(&line_bytes[..line_bytes.len() - 1])
                .unwrap_or("")
                .trim();
            if line_str.is_empty() {
                continue;
            }
            let msg: NtfyMessage = match serde_json::from_str(line_str) {
                Ok(m) => m,
                Err(e) => {
                    tracing::debug!(
                        target: "ntfy_inbound",
                        "failed to parse ntfy line (ignored): {e}: {line_str}"
                    );
                    continue;
                }
            };
            if msg.event != "message" {
                // keepalive, open, poll_request — ignore.
                continue;
            }
            let sender = format!("ntfy:{}", cfg.topic);
            if let Err(e) = service.record_channel_inbound(
                &cfg.channel_id,
                &sender,
                &msg.title,
                &msg.message,
                &msg.tags,
            ) {
                tracing::warn!(
                    target: "ntfy_inbound",
                    "failed to record inbound message: {e}"
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ntfy_message_line() {
        let line = r#"{"id":"abc123","time":1714367400,"event":"message","topic":"asylum-test","message":"hello world","title":"approve","tags":["tag1","tag2"]}"#;
        let msg: NtfyMessage = serde_json::from_str(line).expect("should parse");
        assert_eq!(msg.event, "message");
        assert_eq!(msg.message, "hello world");
        assert_eq!(msg.title, "approve");
        assert_eq!(msg.tags, vec!["tag1", "tag2"]);
        assert_eq!(msg.id, "abc123");
        assert_eq!(msg.topic, "asylum-test");
    }

    #[test]
    fn parse_ntfy_keepalive_line() {
        let line = r#"{"id":"k1","time":1714367400,"event":"keepalive","topic":"asylum-test"}"#;
        let msg: NtfyMessage = serde_json::from_str(line).expect("should parse keepalive");
        assert_eq!(msg.event, "keepalive");
        assert_eq!(msg.message, "");
        assert_eq!(msg.tags, Vec::<String>::new());
    }

    #[test]
    fn parse_ntfy_open_line() {
        let line = r#"{"id":"o1","time":1714367400,"event":"open","topic":"asylum-test"}"#;
        let msg: NtfyMessage = serde_json::from_str(line).expect("should parse open event");
        assert_eq!(msg.event, "open");
    }

    #[test]
    fn parse_ntfy_minimal_message() {
        // ntfy sometimes omits title/tags for simple messages
        let line = r#"{"id":"m1","time":1714367400,"event":"message","topic":"t","message":"ping"}"#;
        let msg: NtfyMessage = serde_json::from_str(line).expect("should parse minimal message");
        assert_eq!(msg.event, "message");
        assert_eq!(msg.message, "ping");
        assert_eq!(msg.title, "");
        assert!(msg.tags.is_empty());
    }
}
