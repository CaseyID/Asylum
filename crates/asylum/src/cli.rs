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
        Command::Serve { bind, database } => {
            asylum_daemon::app::serve(bind, database).await?;
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
        #[arg(long, default_value = "127.0.0.1:7717")]
        bind: SocketAddr,
        #[arg(long, default_value = ".asylum/asylum.sqlite3")]
        database: String,
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
}
