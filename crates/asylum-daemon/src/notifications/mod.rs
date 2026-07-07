use anyhow::Result;

pub use self::ntfy::NtfyConfig;

pub mod ntfy {
    use anyhow::{anyhow, Result};
    use reqwest::Client;
    use serde_json::json;

    #[derive(Clone, Debug)]
    pub struct NtfyConfig {
        pub server: String,
        pub topic: String,
        pub token: Option<String>,
    }

    #[derive(Clone, Debug)]
    pub struct NtfyOutbound {
        pub client: Client,
        pub config: NtfyConfig,
    }

    impl NtfyOutbound {
        pub fn new(
            server: impl Into<String>,
            topic: impl Into<String>,
            token: Option<String>,
        ) -> Self {
            Self {
                client: Client::new(),
                config: NtfyConfig {
                    server: server.into(),
                    topic: topic.into(),
                    token,
                },
            }
        }

        pub async fn send_notification(
            &self,
            title: &str,
            body: &str,
            tags: &[&str],
            priority: i32,
        ) -> Result<()> {
            // ntfy JSON publishing posts to the server ROOT with `topic` in the
            // body. Posting a JSON body to `/{topic}` instead makes ntfy treat the
            // whole JSON string as the raw message (title/tags/priority ignored and
            // any trailing reply marker buried mid-message), so publish to root.
            let endpoint = self.config.server.trim_end_matches('/').to_string();
            let payload = json!({
                "topic": self.config.topic,
                "title": title,
                "message": body,
                "tags": tags,
                "priority": priority,
            });
            let mut request = self.client.post(endpoint).json(&payload);
            if let Some(token) = &self.config.token {
                request = request.header("Authorization", format!("Bearer {}", token));
            }
            let response = request.send().await?;
            if !response.status().is_success() {
                return Err(anyhow!("ntfy send failed: {}", response.status()));
            }
            Ok(())
        }
    }
}

pub async fn send_with_optional_config(
    config: Option<&asylum_types::config::NtfyConfig>,
    title: &str,
    body: &str,
) -> Result<bool> {
    let config = match config {
        Some(cfg) => cfg,
        None => return Ok(false),
    };
    let server = config
        .server
        .clone()
        .unwrap_or_else(|| "https://ntfy.sh".to_string());
    let topic = config.topic.clone().unwrap_or_else(|| "asylum".to_string());
    let token = config.token.clone();
    let client = ntfy::NtfyOutbound::new(server, topic, token);
    client.send_notification(title, body, &[], 3).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::ntfy::NtfyOutbound;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// One-shot capture server: accepts a single connection, returns the raw
    /// request bytes, and answers 200 so the client is satisfied.
    async fn capture_once(listener: tokio::net::TcpListener) -> String {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut buf = vec![0u8; 16384];
        let n = stream.read(&mut buf).await.expect("read request");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .expect("write response");
        let _ = stream.shutdown().await;
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }

    /// ntfy JSON publishing must POST to the server ROOT with the topic in the
    /// JSON body. Posting to `/{topic}` makes ntfy treat the JSON body as the
    /// raw message text (title/tags/priority ignored, reply markers mangled) —
    /// the exact bug found in the Phase B live gate.
    #[tokio::test]
    async fn send_notification_posts_json_to_server_root_with_topic_in_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("local_addr").port();
        let server = tokio::spawn(capture_once(listener));

        let outbound = NtfyOutbound::new(format!("http://127.0.0.1:{port}"), "topic-x", None);
        outbound
            .send_notification("Asylum decision", "Decision needed\n\n[asylum-reply:abc12]", &[], 3)
            .await
            .expect("send");

        let request = server.await.expect("join");
        let request_line = request.lines().next().unwrap_or_default().to_string();
        assert_eq!(
            request_line, "POST / HTTP/1.1",
            "must publish to the server root, not /topic-x; got: {request_line}"
        );
        let body_start = request.find("\r\n\r\n").expect("header/body split") + 4;
        let body: serde_json::Value =
            serde_json::from_str(&request[body_start..]).expect("json body");
        assert_eq!(body["topic"], serde_json::json!("topic-x"));
        assert_eq!(body["title"], serde_json::json!("Asylum decision"));
        assert_eq!(
            body["message"],
            serde_json::json!("Decision needed\n\n[asylum-reply:abc12]")
        );
        assert_eq!(body["priority"], serde_json::json!(3));
    }
}
