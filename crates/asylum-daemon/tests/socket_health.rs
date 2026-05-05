use std::net::SocketAddr;

use anyhow::{Context, Result};
use asylum_types::api::HealthResponse;
use asylum_types::config::AsylumConfig;

#[tokio::test]
async fn unix_socket_health_bypasses_http_owner_token_auth() -> Result<()> {
    let tempdir = tempfile::tempdir()?;
    let database = tempdir.path().join("asylum.sqlite3").display().to_string();
    let socket_path = tempdir.path().join("run").join("asylum.sock");

    let mut config = AsylumConfig::default();
    config.auth.owner_tokens_enabled = true;
    config.auth.owner_token = Some("http-token-required".to_string());

    let bind: SocketAddr = "127.0.0.1:0".parse()?;
    let socket_for_task = socket_path.clone();
    let handle = tokio::spawn(async move {
        asylum_daemon::app::serve_with_socket(bind, database, Some(socket_for_task), config).await
    });

    let client = reqwest::Client::builder()
        .unix_socket(socket_path.clone())
        .build()
        .context("build socket client")?;

    let health = poll_health(&client).await?;
    assert_eq!(health.status, "ok");
    assert_eq!(
        health.socket_path.as_deref(),
        Some(socket_path.to_str().unwrap())
    );

    handle.abort();
    Ok(())
}

async fn poll_health(client: &reqwest::Client) -> Result<HealthResponse> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match client
            .get("http://asylum.local/api/health")
            .send()
            .await
            .and_then(|response| response.error_for_status())
        {
            Ok(response) => return Ok(response.json().await?),
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
}
