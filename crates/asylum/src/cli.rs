use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::AsylumClient;
use crate::mcp;
use crate::native_attach::format_native_attach_prompt;
use asylum_core::api::CreateNodeRequest;
use asylum_core::config::AsylumConfig;
use asylum_core::security::TokenRequest;

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let client = AsylumClient::from_env();
    match cli.command {
        Command::Serve { serve } => {
            let ServeState {
                bind,
                database,
                config,
            } = load_serve_config(serve)?;
            asylum_daemon::app::serve(bind, database, config).await?;
        }
        Command::Config { command: config } => match config {
            ConfigCommand::Init => run_config_init()?,
            ConfigCommand::Show => run_config_show()?,
        },
        Command::Install { command: install } => {
            let payload = match install {
                InstallCommand::Launchd => launchd_plist(),
                InstallCommand::Systemd => systemd_unit()?,
            };
            println!("{payload}");
        }
        Command::Node { command: node } => match node {
            NodeCommand::Create(request) => {
                let response = client.create_node(request.into_request()).await?;
                println!("created node {}", response.node_id);
            }
            NodeCommand::List => {
                let response = client.list_nodes().await?;
                println!("{}", serde_json::to_string_pretty(&response.nodes)?);
            }
            NodeCommand::Inspect { node_id } => {
                let response = client.inspect_node(node_id).await?;
                println!("{}", serde_json::to_string_pretty(&response.node)?);
            }
            NodeCommand::Send { node_id, text } => {
                client.send_input(node_id, text).await?;
                println!("input sent");
            }
            NodeCommand::Interrupt { node_id } => {
                client.interrupt_node(node_id).await?;
                println!("node interrupted");
            }
            NodeCommand::Stop { node_id } => {
                client.stop_node(node_id).await?;
                println!("node stopped");
            }
            NodeCommand::Archive { node_id } => {
                client.archive_node(node_id).await?;
                println!("node archived");
            }
        },
        Command::Graph {
            command: GraphCommand::Get,
        } => {
            let response = client.graph().await?;
            println!("{}", serde_json::to_string_pretty(&response.graph)?);
        }
        Command::Attach { node_id } => {
            let target = client.native_attach_target(node_id).await?;
            println!("{label}", label = target.label);
            println!("{}", format_native_attach_prompt(&target));
        }
        Command::Token { command: token } => match token {
            TokenCommand::Issue {
                name,
                scope,
                ttl_seconds,
            } => {
                let request = TokenRequest {
                    name,
                    scope,
                    ttl_seconds,
                };
                let response = client.issue_token(request).await?;
                println!("issued token: {}", response.raw_token);
                println!("id: {}", response.id);
                println!("expires_at_epoch_secs: {}", response.expires_at_epoch_secs);
            }
        },
        Command::Notify { command: notify } => match notify {
            NotifyCommand::Send { title, body } => {
                let sent = client.notify_send(title, body).await?;
                println!("notify sent: {sent}");
            }
        },
        Command::Mcp => {
            mcp::run_stdio_server(Arc::new(client)).await?;
        }
    }

    Ok(())
}

#[derive(Parser)]
#[command(name = "asylum", version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve {
        #[command(flatten)]
        serve: ServeConfig,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Install {
        #[command(subcommand)]
        command: InstallCommand,
    },
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    Attach {
        node_id: Uuid,
    },
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
    Notify {
        #[command(subcommand)]
        command: NotifyCommand,
    },
    Mcp,
}

#[derive(Args)]
struct ServeConfig {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    bind: Option<SocketAddr>,
    #[arg(long)]
    database: Option<String>,
    #[arg(long)]
    base_url: Option<String>,
    #[arg(long, value_name = "VALUE")]
    owner_token: Option<String>,
    #[arg(long)]
    owner_tokens_enabled: bool,
    #[arg(long)]
    ntfy_server: Option<String>,
    #[arg(long)]
    ntfy_topic: Option<String>,
    #[arg(long)]
    ntfy_token: Option<String>,
    #[arg(long)]
    loon_enabled: bool,
    #[arg(long)]
    loon_endpoint: Option<String>,
    #[arg(long)]
    loon_cli_path: Option<PathBuf>,
    #[arg(long)]
    harness_codex_command: Option<String>,
    #[arg(long)]
    harness_claude_command: Option<String>,
    #[arg(long)]
    workspace_recent_limit: Option<usize>,
}

struct ServeState {
    bind: SocketAddr,
    database: String,
    config: AsylumConfig,
}

#[derive(Subcommand)]
enum ConfigCommand {
    Init,
    Show,
}

#[derive(Subcommand)]
enum InstallCommand {
    Launchd,
    Systemd,
}

#[derive(Subcommand)]
enum GraphCommand {
    Get,
}

#[derive(Subcommand)]
enum TokenCommand {
    Issue {
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Vec<String>,
        #[arg(long)]
        ttl_seconds: Option<u64>,
    },
}

#[derive(Subcommand)]
enum NotifyCommand {
    Send {
        #[arg(long)]
        title: String,
        #[arg(long)]
        body: String,
    },
}

#[derive(Subcommand)]
enum NodeCommand {
    Create(NodeCreateArgs),
    List,
    Inspect { node_id: Uuid },
    Send { node_id: Uuid, text: String },
    Interrupt { node_id: Uuid },
    Stop { node_id: Uuid },
    Archive { node_id: Uuid },
}

#[derive(Args)]
struct NodeCreateArgs {
    #[arg(long)]
    harness: String,
    #[arg(long)]
    substrate: String,
    #[arg(long, alias = "role_hint", default_value = "worker")]
    role: String,
    #[arg(long)]
    workspace: Option<String>,
    #[arg(long)]
    description: Option<String>,
}

impl NodeCreateArgs {
    fn into_request(self) -> CreateNodeRequest {
        CreateNodeRequest {
            harness: self.harness,
            substrate: self.substrate,
            role_hint: self.role,
            workspace: self.workspace,
            description: self.description,
            created_by: None,
            launch_args: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ConfigFile {
    #[serde(flatten)]
    core: AsylumConfig,
    database: String,
}

impl Default for ConfigFile {
    fn default() -> Self {
        let mut config = AsylumConfig::default();
        config.ntfy = asylum_core::config::NtfyConfig {
            server: None,
            topic: None,
            token: None,
            poll_interval_seconds: 30,
        };

        Self {
            core: config,
            database: ".asylum/asylum.sqlite3".to_string(),
        }
    }
}

fn config_path() -> Result<PathBuf> {
    let mut base = if let Some(path) = dirs::config_dir() {
        path
    } else if let Ok(home) = env::var("HOME") {
        Path::new(&home).join(".config")
    } else {
        Path::new(".").to_path_buf()
    };
    base.push("asylum");
    base.push("config.toml");
    Ok(base)
}

fn parse_bool_flag(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn load_config_file(path: Option<PathBuf>) -> Result<ConfigFile> {
    let path = match path {
        Some(path) => path,
        None => {
            let default = config_path()?;
            if !default.exists() {
                return Ok(ConfigFile::default());
            }
            default
        }
    };

    if !path.exists() {
        return Ok(ConfigFile::default());
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("read config file {}", path.display()))?;
    toml::from_str::<ConfigFile>(&content).with_context(|| format!("parse config file {path:?}"))
}

fn apply_env_overrides(config: &mut AsylumConfig) {
    if let Ok(value) = env::var("ASYLUM_BASE_URL") {
        config.base_url = value;
    }
    if let Ok(value) = env::var("ASYLUM_OWNER_TOKEN") {
        config.auth.owner_token = Some(value);
    }
    if let Ok(value) = env::var("ASYLUM_OWNER_TOKENS_ENABLED") {
        config.auth.owner_tokens_enabled = parse_bool_flag(&value);
    }
    if let Ok(value) = env::var("ASYLUM_NTFY_SERVER") {
        config.ntfy.server = Some(value);
    }
    if let Ok(value) = env::var("ASYLUM_NTFY_TOPIC") {
        config.ntfy.topic = Some(value);
    }
    if let Ok(value) = env::var("ASYLUM_NTFY_TOKEN") {
        config.ntfy.token = Some(value);
    }
    if let Ok(value) = env::var("ASYLUM_LOON_ENABLED") {
        config.loon.enabled = parse_bool_flag(&value);
    }
    if let Ok(value) = env::var("ASYLUM_LOON_ENDPOINT") {
        config.loon.endpoint = value;
    }
    if let Ok(value) = env::var("ASYLUM_HARNESS_CODEX_COMMAND") {
        config.harness.codex_command = value;
    }
    if let Ok(value) = env::var("ASYLUM_HARNESS_CLAUDE_COMMAND") {
        config.harness.claude_command = value;
    }
    if let Ok(value) = env::var("ASYLUM_WORKSPACE_RECENT_LIMIT") {
        if let Ok(recent_limit) = value.parse::<usize>() {
            config.workspace.recent_limit = recent_limit;
        }
    }
}

fn apply_cli_overrides(config: &mut AsylumConfig, args: &ServeConfig) {
    if let Some(base_url) = args.base_url.as_deref() {
        config.base_url = base_url.to_string();
    }
    if args.owner_tokens_enabled {
        config.auth.owner_tokens_enabled = true;
    }
    if let Some(owner_token) = args.owner_token.as_deref() {
        config.auth.owner_token = Some(owner_token.to_string());
        config.auth.owner_tokens_enabled = true;
    }
    if let Some(ntfy_server) = args.ntfy_server.as_deref() {
        config.ntfy.server = Some(ntfy_server.to_string());
    }
    if let Some(ntfy_topic) = args.ntfy_topic.as_deref() {
        config.ntfy.topic = Some(ntfy_topic.to_string());
    }
    if let Some(ntfy_token) = args.ntfy_token.as_deref() {
        config.ntfy.token = Some(ntfy_token.to_string());
    }
    if args.loon_enabled {
        config.loon.enabled = true;
    }
    if let Some(loon_endpoint) = args.loon_endpoint.as_deref() {
        config.loon.endpoint = loon_endpoint.to_string();
    }
    if let Some(cli_path) = &args.loon_cli_path {
        config.loon.cli_path = Some(cli_path.clone());
    }
    if let Some(command) = args.harness_codex_command.as_deref() {
        config.harness.codex_command = command.to_string();
    }
    if let Some(command) = args.harness_claude_command.as_deref() {
        config.harness.claude_command = command.to_string();
    }
    if let Some(recent_limit) = args.workspace_recent_limit {
        config.workspace.recent_limit = recent_limit;
    }
}

fn load_serve_config(args: ServeConfig) -> Result<ServeState> {
    let file_path = args.config.clone();
    let file_config = load_config_file(file_path)?;
    let mut config = file_config.core;

    if let Ok(bind_override) = env::var("ASYLUM_BIND") {
        if let Ok(bind) = bind_override.parse::<SocketAddr>() {
            config.listen = Some(bind.to_string());
        }
    }
    apply_env_overrides(&mut config);
    apply_cli_overrides(&mut config, &args);

    let bind = args
        .bind
        .or_else(|| config.listen.as_ref().and_then(|value| value.parse().ok()))
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 7717)));
    let file_database = file_config.database;
    let database = args.database.unwrap_or(file_database);

    Ok(ServeState {
        bind,
        database,
        config,
    })
}

fn run_config_init() -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let config = ConfigFile::default();
    let content = toml::to_string_pretty(&config).context("serialize config")?;
    std::fs::write(&path, content).context("write config file")?;
    println!("wrote config to {}", path.display());
    Ok(())
}

fn run_config_show() -> Result<()> {
    let path = config_path()?;
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("read config file {}", path.display()))?;
    println!("{content}");
    Ok(())
}

fn launchd_plist() -> String {
    let binary = env::current_exe()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .unwrap_or_else(|| "/usr/local/bin/asylum".to_string());
    let database = PathBuf::from("$HOME/.asylum/asylum.sqlite3");
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
            "<plist version=\"1.0\">\n",
            "  <dict>\n",
            "    <key>Label</key>\n",
            "    <string>dev.asylum.daemon</string>\n",
            "    <key>ProgramArguments</key>\n",
            "    <array>\n",
            "      <string>{0}</string>\n",
            "      <string>serve</string>\n",
            "      <string>--database</string>\n",
            "      <string>{1}</string>\n",
            "    </array>\n",
            "    <key>RunAtLoad</key>\n",
            "    <true/>\n",
            "    <key>KeepAlive</key>\n",
            "    <true/>\n",
            "  </dict>\n",
            "</plist>\n",
        ),
        binary,
        database.display()
    )
}

fn systemd_unit() -> Result<String> {
    let binary = env::current_exe()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .context("locate asylum executable")?;
    Ok(format!(
        concat!(
            "[Unit]\n",
            "Description=Asylum Daemon\n",
            "After=network-online.target\n\n",
            "[Service]\n",
            "Type=simple\n",
            "ExecStart={0} serve --database {1}\n",
            "Restart=on-failure\n",
            "RestartSec=3\n",
            "WorkingDirectory=%h\n\n",
            "[Install]\n",
            "WantedBy=default.target\n",
        ),
        binary, "~/.asylum/asylum.sqlite3"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_alias_in_command_structures_is_available() {
        let args = NodeCreateArgs {
            harness: "codex".to_string(),
            substrate: "local".to_string(),
            role: "command-center".to_string(),
            workspace: Some(".".to_string()),
            description: None,
        };
        let request = args.into_request();
        assert_eq!(request.harness, "codex");
        assert_eq!(request.role_hint, "command-center");
        assert_eq!(request.substrate, "local");
    }

    #[test]
    fn serve_config_merges_cli_overrides_with_file() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let config_path = tempdir.path().join("config.toml");
        let mut file = ConfigFile::default();
        file.core.base_url = "http://from-file".to_string();
        file.core.workspace.recent_limit = 5;
        file.database = ".asylum/asylum.sqlite3".to_string();
        file.core.ntfy.server = Some("https://from-file".to_string());
        std::fs::write(
            &config_path,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;

        let args = ServeConfig {
            config: Some(config_path.clone()),
            bind: Some("127.0.0.1:9000".parse()?),
            database: None,
            base_url: Some("http://from-cli".to_string()),
            owner_token: None,
            owner_tokens_enabled: false,
            ntfy_server: Some("https://from-cli".to_string()),
            ntfy_topic: None,
            ntfy_token: None,
            loon_enabled: true,
            loon_endpoint: Some("http://loon".to_string()),
            loon_cli_path: None,
            harness_codex_command: None,
            harness_claude_command: Some("claude-cli".to_string()),
            workspace_recent_limit: Some(11),
        };

        let ServeState {
            bind,
            database,
            config,
        } = load_serve_config(args)?;

        assert_eq!(bind.to_string(), "127.0.0.1:9000");
        assert_eq!(database, ".asylum/asylum.sqlite3");
        assert_eq!(config.base_url, "http://from-cli");
        assert_eq!(config.ntfy.server, Some("https://from-cli".to_string()));
        assert!(config.ntfy.topic.is_none());
        assert_eq!(config.workspace.recent_limit, 11);
        assert_eq!(config.harness.claude_command, "claude-cli");
        assert_eq!(config.loon.endpoint, "http://loon");
        assert!(config.loon.enabled);

        Ok(())
    }

    #[test]
    fn serve_config_reads_owner_token_env() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let config_path = tempdir.path().join("config.toml");
        let file = ConfigFile::default();
        std::fs::write(
            &config_path,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;

        let prev_token = std::env::var_os("ASYLUM_OWNER_TOKEN");
        let prev_enabled = std::env::var_os("ASYLUM_OWNER_TOKENS_ENABLED");
        std::env::set_var("ASYLUM_OWNER_TOKEN", "env-owner");
        std::env::set_var("ASYLUM_OWNER_TOKENS_ENABLED", "true");

        let args = ServeConfig {
            config: Some(config_path.clone()),
            bind: None,
            database: None,
            base_url: None,
            owner_token: None,
            owner_tokens_enabled: false,
            ntfy_server: None,
            ntfy_topic: None,
            ntfy_token: None,
            loon_enabled: false,
            loon_endpoint: None,
            loon_cli_path: None,
            harness_codex_command: None,
            harness_claude_command: None,
            workspace_recent_limit: None,
        };

        let ServeState { config, .. } = load_serve_config(args)?;
        assert_eq!(config.auth.owner_token.as_deref(), Some("env-owner"));
        assert!(config.auth.owner_tokens_enabled);

        if let Some(value) = prev_token {
            std::env::set_var("ASYLUM_OWNER_TOKEN", value);
        } else {
            std::env::remove_var("ASYLUM_OWNER_TOKEN");
        }
        if let Some(value) = prev_enabled {
            std::env::set_var("ASYLUM_OWNER_TOKENS_ENABLED", value);
        } else {
            std::env::remove_var("ASYLUM_OWNER_TOKENS_ENABLED");
        }
        Ok(())
    }
}
