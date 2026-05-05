use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    match asylum_cli::parse()? {
        asylum_cli::TopLevelAction::Cli(action) => asylum_cli::run(action).await,
        asylum_cli::TopLevelAction::DaemonRun { config, options } => {
            asylum_daemon::run(asylum_daemon::DaemonRunOptions {
                config,
                bind: options.bind,
                database: options.database,
                socket_path: options.socket_path,
                base_url: options.base_url,
                owner_token: options.owner_token,
                owner_tokens_enabled: options.owner_tokens_enabled,
                ntfy_server: options.ntfy_server,
                ntfy_topic: options.ntfy_topic,
                ntfy_token: options.ntfy_token,
                loon_enabled: options.loon_enabled,
                loon_endpoint: options.loon_endpoint,
                loon_cli_path: options.loon_cli_path,
                harness_codex_command: options.harness_codex_command,
                harness_claude_command: options.harness_claude_command,
                workspace_recent_limit: options.workspace_recent_limit,
            })
            .await
        }
    }
}
