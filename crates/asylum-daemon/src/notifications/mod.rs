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
            let endpoint = format!(
                "{}/{}",
                self.config.server.trim_end_matches('/'),
                self.config.topic
            );
            let payload = json!({
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
    config: Option<&asylum_core::config::NtfyConfig>,
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
