use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::AsylumClient;
use crate::host::{
    command_exists, config_dir_path, require_binary, service_state_from_health, HostState,
    PortInUse, ServiceBackend, ServiceManager, ServiceState,
};
use crate::mcp;
use crate::native_attach::format_native_attach_prompt;
use crate::runtime::RuntimePaths;
use asylum_core::api::{CreateNodeRequest, HealthResponse};
use asylum_core::config::AsylumConfig;
use asylum_core::security::TokenRequest;

const DEFAULT_BIND: &str = "127.0.0.1:7717";
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:7717";
const PUBLIC_INSTALLER_URL: &str =
    "https://raw.githubusercontent.com/CaseyID/Asylum/main/scripts/install.sh";

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let was_bare = cli.command.is_none();
    let paths = RuntimePaths::from_env(cli.config.clone())?;
    let client = runtime_client(&paths)?;

    if was_bare && !paths.home.exists() {
        // First run: print a friendly intro instead of barreling into the cockpit.
        run_first_run_banner(&paths);
        return Ok(());
    }

    let command = cli.command.unwrap_or(Command::Cockpit);

    match command {
        Command::Setup => run_setup(&paths)?,
        Command::Cockpit => {
            let client = runtime_client(&paths)?;
            run_cockpit(&paths, &client, was_bare).await?
        }
        Command::Start => {
            let client = runtime_client(&paths)?;
            let _ = run_start(&paths, &client).await?;
        }
        Command::Stop => run_stop(&paths)?,
        Command::Restart => {
            let client = runtime_client(&paths)?;
            run_restart(&paths, &client).await?
        }
        Command::Status { json } => {
            let client = runtime_client(&paths)?;
            run_status(&paths, &client, json).await?
        }
        Command::Doctor { verbose } => {
            let client = runtime_client(&paths)?;
            run_doctor(&paths, &client, verbose).await?
        }
        Command::Logs { tail } => run_logs(&paths, tail)?,
        Command::Update { version } => {
            let client = runtime_client(&paths)?;
            run_update(&paths, &client, version).await?
        }
        Command::Serve { serve } => {
            let ServeState {
                bind,
                database,
                config,
            } = load_serve_config(serve, &paths)?;
            asylum_daemon::app::serve(bind, database, config).await?;
        }
        Command::Config { command: config } => match config {
            ConfigCommand::Init => run_config_init(&paths)?,
            ConfigCommand::Show => run_config_show(&paths)?,
        },
        Command::Service { command: service } => match service {
            ServiceCommand::Generate { command: generate } => {
                let manager = ServiceManager::new(paths.clone())?;
                let payload = match generate {
                    ServiceGenerateCommand::Launchd => manager.launchd_plist_text(DEFAULT_BIND),
                    ServiceGenerateCommand::Systemd => manager.systemd_unit_text(DEFAULT_BIND),
                };
                println!("{payload}");
            }
        },
        Command::Uninstall(args) => {
            let client = runtime_client(&paths)?;
            run_uninstall(&paths, &client, args).await?;
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
#[command(name = "asylum", version)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Idempotent post-install dance; creates ~/.asylum, writes default config.
    #[command(after_help = "Examples:\n  asylum setup\n  ASYLUM_HOME=/tmp/asylum-test asylum setup")]
    Setup,
    /// Open the Cockpit in your browser; starts the daemon if needed.
    #[command(after_help = "Examples:\n  asylum cockpit\n  asylum   # bare invocation falls through to cockpit when set up")]
    Cockpit,
    /// Start the daemon (background; uses launchd / systemd-user / pid fallback).
    #[command(after_help = "Examples:\n  asylum start\n  asylum start && asylum status")]
    Start,
    /// Stop the daemon if running.
    #[command(after_help = "Examples:\n  asylum stop\n  asylum stop && asylum status")]
    Stop,
    Restart,
    /// Show host and daemon state.
    #[command(after_help = "Examples:\n  asylum status\n  asylum status --json\n  asylum status --json | jq .daemon")]
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Run host checks and report problems.
    #[command(after_help = "Examples:\n  asylum doctor\n  asylum doctor --verbose")]
    Doctor {
        #[arg(long)]
        verbose: bool,
    },
    Logs {
        #[arg(long)]
        tail: bool,
    },
    /// Self-update via the public installer.
    #[command(after_help = "Examples:\n  asylum update\n  asylum update --version v0.1.3")]
    Update {
        #[arg(long)]
        version: Option<String>,
    },
    Serve {
        #[command(flatten)]
        serve: ServeConfig,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Service-unit operations (generate launchd / systemd-user unit text).
    #[command(after_help = "Examples:\n  asylum service generate systemd > ~/.config/systemd/user/asylum.service\n  asylum service generate launchd > ~/Library/LaunchAgents/dev.asylum.daemon.plist")]
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Remove Asylum from this machine. Stops daemon, removes service unit, binary, ~/.asylum.
    #[command(after_help = "Examples:\n  asylum uninstall --dry-run\n  asylum uninstall --keep state\n  asylum uninstall --purge   # also removes ~/.config/asylum (signing keys)")]
    Uninstall(UninstallArgs),
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    /// Attach a TTY directly to a node's harness.
    #[command(after_help = "Examples:\n  asylum attach <NODE_ID>\n  asylum attach $(asylum node list | jq -r '.[0].node_id')")]
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
enum ServiceCommand {
    /// Generate a service-unit file for the host's launch system.
    #[command(after_help = "Examples:\n  asylum service generate systemd\n  asylum service generate launchd")]
    Generate {
        #[command(subcommand)]
        command: ServiceGenerateCommand,
    },
}

#[derive(Subcommand)]
enum ServiceGenerateCommand {
    Launchd,
    Systemd,
}

#[derive(Args, Debug)]
struct UninstallArgs {
    /// Skip the interactive y/n confirm. Does NOT skip --purge typed-name confirmation.
    #[arg(long)]
    yes: bool,
    /// Preserve the listed paths. Repeatable: `--keep state --keep logs`.
    #[arg(long, value_enum)]
    keep: Vec<KeepKind>,
    /// Also remove ~/.config/asylum (signing keys). Requires typed-name confirm.
    #[arg(long)]
    purge: bool,
    /// Print the plan and exit without removing anything.
    #[arg(long)]
    dry_run: bool,
    /// Emit the plan (and post-state, if executed) as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
enum KeepKind {
    /// Preserve ~/.asylum (state, sqlite, logs, sockets).
    State,
    /// Preserve ~/.config/asylum (signing keys etc.). This is the default; the flag
    /// is for explicitness in scripts.
    Config,
    /// Preserve ~/.asylum/logs/ even if state is removed.
    Logs,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(flatten)]
    core: AsylumConfig,
    database: String,
}

impl ConfigFile {
    fn default_for_paths(paths: &RuntimePaths) -> Self {
        let mut config = AsylumConfig::default();
        config.listen = Some(DEFAULT_BIND.to_string());
        config.base_url = DEFAULT_BASE_URL.to_string();
        config.ntfy = asylum_core::config::NtfyConfig {
            server: None,
            topic: None,
            token: None,
            poll_interval_seconds: 30,
        };
        config.harness.codex_command = detected_command("codex");
        config.harness.claude_command = detected_command("claude");

        Self {
            core: config,
            database: paths.database.display().to_string(),
        }
    }
}

impl Default for ConfigFile {
    fn default() -> Self {
        let paths = RuntimePaths::from_values(
            env::var_os("ASYLUM_HOME").map(PathBuf::from),
            env::var_os("ASYLUM_CONFIG").map(PathBuf::from),
            env::var_os("ASYLUM_DATABASE").map(PathBuf::from),
            env::var_os("HOME").map(PathBuf::from),
        );
        Self::default_for_paths(&paths)
    }
}

#[derive(Debug)]
struct SetupOutcome {
    created_config: bool,
    config_path: PathBuf,
    database_path: PathBuf,
    codex_available: bool,
    claude_available: bool,
}

fn detected_command(command: &str) -> String {
    command.to_string()
}

fn setup_runtime(paths: &RuntimePaths) -> Result<SetupOutcome> {
    paths.ensure_dirs()?;
    let created_config = if paths.config.exists() {
        false
    } else {
        let config = ConfigFile::default_for_paths(paths);
        let content = toml::to_string_pretty(&config).context("serialize config")?;
        fs::write(&paths.config, content).context("write config file")?;
        true
    };

    Ok(SetupOutcome {
        created_config,
        config_path: paths.config.clone(),
        database_path: paths.database.clone(),
        codex_available: command_exists("codex"),
        claude_available: command_exists("claude"),
    })
}

fn run_setup(paths: &RuntimePaths) -> Result<()> {
    let pre_state = HostState::collect(paths);
    let was_first_run = !pre_state.runtime_dir.present;

    let outcome = setup_runtime(paths)?;
    println!("Asylum home: {}", paths.home.display());
    if outcome.created_config {
        println!("Created config: {}", outcome.config_path.display());
    } else {
        println!("Config already exists: {}", outcome.config_path.display());
    }
    println!("Database: {}", outcome.database_path.display());
    print_harness_detection("Codex", "codex", outcome.codex_available);
    print_harness_detection("Claude Code", "claude", outcome.claude_available);
    println!("Loon: optional; enable it in config when ready");
    println!("ntfy: optional; set server/topic in config when ready");

    if was_first_run {
        print_first_run_banner(paths);
    } else {
        println!("Next: asylum");
    }
    Ok(())
}

fn print_first_run_banner(paths: &RuntimePaths) {
    let post_state = HostState::collect(paths);
    println!();
    println!("Welcome to Asylum.");
    println!();
    println!("Asylum runs as a small daemon on this machine, with a local web Cockpit");
    println!("for observing and steering harness sessions (Codex, Claude Code).");
    println!();
    if !post_state.binary.on_path {
        if let Some(bin) = &post_state.binary.path {
            if let Some(install_dir) = bin.parent() {
                println!("NOTE: `asylum` is not yet on your shell PATH.");
                println!("      Add this line to your shell rc and restart the shell:");
                println!("          export PATH=\"{}:$PATH\"", install_dir.display());
                println!();
            }
        }
    }
    println!("What's next:");
    println!("  asylum start    # start the daemon");
    println!("  asylum cockpit  # open the Cockpit in your browser");
    println!("  asylum status   # see what's running");
    println!("  asylum doctor   # run host checks");
    println!();
}

fn run_first_run_banner(paths: &RuntimePaths) {
    println!("Asylum is installed but not yet set up on this machine.");
    println!();
    println!("Asylum home would live at: {}", paths.home.display());
    println!();
    println!("Run `asylum setup` to create it and finish first-time configuration.");
    println!();
    println!("Other useful commands:");
    println!("  asylum status   # show host state (works without setup)");
    println!("  asylum doctor   # run host checks");
    println!("  asylum --help   # full command list");
}

fn print_harness_detection(label: &str, command: &str, available: bool) {
    if available {
        println!("{label}: found `{command}` on PATH");
    } else {
        println!("{label}: `{command}` not found on PATH");
    }
}

fn parse_bool_flag(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn load_config_file(paths: &RuntimePaths) -> Result<ConfigFile> {
    if !paths.config.exists() {
        return Ok(ConfigFile::default_for_paths(paths));
    }

    let content = fs::read_to_string(&paths.config)
        .with_context(|| format!("read config file {}", paths.config.display()))?;
    toml::from_str::<ConfigFile>(&content)
        .with_context(|| format!("parse config file {}", paths.config.display()))
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

fn load_serve_config(args: ServeConfig, paths: &RuntimePaths) -> Result<ServeState> {
    let file_config = load_config_file(paths)?;
    let mut config = file_config.core;
    let base_url_from_file = config.base_url.clone();
    let base_url_overridden = args.base_url.is_some()
        || env::var_os("ASYLUM_BASE_URL").is_some()
        || is_explicit_config_base_url(&base_url_from_file);

    apply_bind_env_override(&mut config);
    apply_env_overrides(&mut config);
    apply_cli_overrides(&mut config, &args);

    let bind = effective_bind(args.bind, &config);
    if !base_url_overridden {
        config.base_url = local_base_url_for_bind(bind);
    }
    let database = args
        .database
        .or_else(|| env::var("ASYLUM_DATABASE").ok())
        .unwrap_or(file_config.database);

    Ok(ServeState {
        bind,
        database,
        config,
    })
}

fn apply_bind_env_override(config: &mut AsylumConfig) {
    if let Ok(bind_override) = env::var("ASYLUM_BIND") {
        if let Ok(bind) = bind_override.parse::<SocketAddr>() {
            config.listen = Some(bind.to_string());
        }
    }
}

fn effective_bind(cli_bind: Option<SocketAddr>, config: &AsylumConfig) -> SocketAddr {
    cli_bind
        .or_else(|| config.listen.as_ref().and_then(|value| value.parse().ok()))
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 7717)))
}

fn effective_service_bind(paths: &RuntimePaths) -> Result<SocketAddr> {
    let mut config = load_config_file(paths)?.core;
    apply_bind_env_override(&mut config);
    Ok(effective_bind(None, &config))
}

fn runtime_client(paths: &RuntimePaths) -> Result<AsylumClient> {
    let bind = effective_service_bind(paths)?;
    Ok(AsylumClient::new(
        effective_runtime_base_url(env::var("ASYLUM_BASE_URL").ok(), bind),
        env::var("ASYLUM_TOKEN").ok(),
    ))
}

fn effective_runtime_base_url(base_url_override: Option<String>, bind: SocketAddr) -> String {
    base_url_override.unwrap_or_else(|| local_base_url_for_bind(bind))
}

fn is_explicit_config_base_url(value: &str) -> bool {
    !value.is_empty() && value != DEFAULT_BASE_URL
}

fn local_base_url_for_bind(bind: SocketAddr) -> String {
    let ip = if bind.ip().is_unspecified() {
        if bind.is_ipv6() {
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        } else {
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        }
    } else {
        bind.ip()
    };
    format!("http://{}", SocketAddr::new(ip, bind.port()))
}

fn run_config_init(paths: &RuntimePaths) -> Result<()> {
    paths.ensure_dirs()?;
    let config = ConfigFile::default_for_paths(paths);
    let content = toml::to_string_pretty(&config).context("serialize config")?;
    fs::write(&paths.config, content).context("write config file")?;
    println!("wrote config to {}", paths.config.display());
    Ok(())
}

fn run_config_show(paths: &RuntimePaths) -> Result<()> {
    let content = fs::read_to_string(&paths.config)
        .with_context(|| format!("read config file {}", paths.config.display()))?;
    println!("{content}");
    Ok(())
}

async fn run_cockpit(paths: &RuntimePaths, client: &AsylumClient, first_run: bool) -> Result<()> {
    if first_run || !paths.config.exists() {
        let _ = setup_runtime(paths)?;
    }
    let start = run_start(paths, client).await?;
    if !start.is_health_ready() {
        return Err(anyhow!(
            "Asylum start requested but health is not ready; run `asylum status` or `asylum logs`."
        ));
    }
    let url = client.base_url().to_string();
    open_browser(&url);
    println!("Cockpit: {url}");
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartOutcome {
    AlreadyHealthy,
    StartedHealthy,
    StartRequestedHealthTimedOut,
}

impl StartOutcome {
    fn is_health_ready(self) -> bool {
        matches!(self, Self::AlreadyHealthy | Self::StartedHealthy)
    }
}

async fn run_start(paths: &RuntimePaths, client: &AsylumClient) -> Result<StartOutcome> {
    if !paths.config.exists() {
        let _ = setup_runtime(paths)?;
    }
    if client.is_healthy().await {
        println!("Asylum is already running at {}", client.base_url());
        return Ok(StartOutcome::AlreadyHealthy);
    }
    let manager = ServiceManager::new(paths.clone())?;
    let bind = effective_service_bind(paths)?;
    manager.start(&bind.to_string())?;
    match wait_for_health(client, Duration::from_secs(6)).await {
        Some(health) => {
            println!(
                "Asylum started at {} ({}, version {})",
                client.base_url(),
                health.status,
                health.daemon_version
            );
            Ok(StartOutcome::StartedHealthy)
        }
        None => {
            println!("Asylum start requested via {}", manager.backend());
            println!("Waiting for health timed out; run `asylum status` or `asylum logs`.");
            Ok(StartOutcome::StartRequestedHealthTimedOut)
        }
    }
}

fn run_stop(paths: &RuntimePaths) -> Result<()> {
    let manager = ServiceManager::new(paths.clone())?;
    manager.stop()?;
    println!("Asylum stop requested");
    Ok(())
}

async fn run_restart(paths: &RuntimePaths, client: &AsylumClient) -> Result<()> {
    let manager = ServiceManager::new(paths.clone())?;
    let bind = effective_service_bind(paths)?;
    manager.restart(&bind.to_string())?;
    if wait_for_health(client, Duration::from_secs(6))
        .await
        .is_some()
    {
        println!("Asylum restarted at {}", client.base_url());
    } else {
        println!("Asylum restart requested; health is not ready yet");
    }
    Ok(())
}

async fn run_status(paths: &RuntimePaths, client: &AsylumClient, json: bool) -> Result<()> {
    let mut state = HostState::collect(paths);
    let health = client.health().await.ok();
    if let Some(health) = health.as_ref() {
        state.daemon.healthy = true;
        state.daemon.state = ServiceState::Running;
        state.daemon.daemon_version = Some(health.daemon_version.clone());
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&state)?);
        return Ok(());
    }

    let service_state = service_state_from_health(health.is_some(), state.daemon.state.clone());
    println!("Asylum: {service_state}");
    println!("Cockpit: {}", client.base_url());
    match health {
        Some(health) => println!("Health: {} (version {})", health.status, health.daemon_version),
        None => println!("Health: unavailable"),
    }
    println!("Config: {}", paths.config.display());
    println!("Database: {}", paths.database.display());
    println!("Logs: {}", paths.log.display());
    println!("Binary: {}", display_optional_path(state.binary.path.as_deref()));
    if !state.binary.shadowed_by.is_empty() {
        println!("PATH shadows:");
        for shadow in &state.binary.shadowed_by {
            println!("  {}", shadow.display());
        }
    }
    println!(
        "Service unit: {} ({})",
        if state.service_unit.installed {
            "installed"
        } else {
            "not installed"
        },
        state.service_unit.backend,
    );
    if let Some(path) = &state.service_unit.path {
        println!("  path: {}", path.display());
    }
    match &state.network.port_in_use {
        PortInUse::Free => println!("Port: free"),
        PortInUse::InUse { pid, command } => {
            let pid_text = pid.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string());
            let command_text = command.as_deref().unwrap_or("?");
            println!("Port: in use (pid {pid_text}, command {command_text})");
        }
        PortInUse::Unknown { reason } => println!("Port: unknown ({reason})"),
    }
    println!(
        "Config dir: {} ({} entries)",
        state.config_dir.path.display(),
        state.config_dir.entry_count
    );
    Ok(())
}

fn display_optional_path(path: Option<&Path>) -> String {
    path.map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string())
}

async fn wait_for_health(client: &AsylumClient, timeout: Duration) -> Option<HealthResponse> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(health) = client.health().await {
            if health.status == "ok" {
                return Some(health);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn open_browser(url: &str) {
    let command = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = ProcessCommand::new(command)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Clone, Debug)]
struct DoctorCheck {
    status: CheckStatus,
    name: &'static str,
    detail: String,
}

impl DoctorCheck {
    fn new(status: CheckStatus, name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status,
            name,
            detail: detail.into(),
        }
    }
}

async fn run_doctor(paths: &RuntimePaths, client: &AsylumClient, verbose: bool) -> Result<()> {
    let checks = doctor_checks(paths, client, verbose).await;
    println!("Asylum doctor");
    for check in &checks {
        println!(
            "{} {:<24} {}",
            check.status.marker(),
            check.name,
            check.detail
        );
    }
    if checks.iter().any(|check| check.status == CheckStatus::Fail) {
        println!("Result: attention needed");
    } else {
        println!("Result: ready");
    }
    Ok(())
}

async fn doctor_checks(
    paths: &RuntimePaths,
    client: &AsylumClient,
    verbose: bool,
) -> Vec<DoctorCheck> {
    let host = HostState::collect(paths);
    let mut checks = Vec::new();
    checks.push(match host.binary.path.as_ref() {
        Some(binary) => DoctorCheck::new(
            CheckStatus::Ok,
            "binary",
            format!("{} {}", binary.display(), host.binary.version),
        ),
        None => match require_binary() {
            Ok(binary) => DoctorCheck::new(
                CheckStatus::Ok,
                "binary",
                format!("{} {}", binary.display(), host.binary.version),
            ),
            Err(error) => DoctorCheck::new(CheckStatus::Fail, "binary", error.to_string()),
        },
    });
    checks.push(classify_required(
        host.binary.on_path,
        "PATH",
        "`asylum` command is available",
        "`asylum` command was not found on PATH",
    ));
    checks.push(path_check("home", &paths.home, true));
    checks.push(path_check("config", &paths.config, false));
    checks.push(database_check(paths));
    checks.push(match client.health().await {
        Ok(health) => DoctorCheck::new(
            CheckStatus::Ok,
            "health",
            format!("{} version {}", health.status, health.daemon_version),
        ),
        Err(_) => DoctorCheck::new(
            CheckStatus::Warn,
            "health",
            "control plane is not responding",
        ),
    });
    checks.push(cockpit_assets_check());
    let config = load_config_file(paths).unwrap_or_else(|_| ConfigFile::default_for_paths(paths));
    checks.push(classify_required(
        command_exists(&config.core.harness.codex_command),
        "codex",
        format!("`{}` found", config.core.harness.codex_command),
        format!("`{}` not found", config.core.harness.codex_command),
    ));
    checks.push(classify_required(
        command_exists(&config.core.harness.claude_command),
        "claude",
        format!("`{}` found", config.core.harness.claude_command),
        format!("`{}` not found", config.core.harness.claude_command),
    ));
    checks.push(optional_check(
        "loon",
        config.core.loon.enabled,
        "enabled",
        "optional and disabled",
    ));
    checks.push(optional_check(
        "ntfy",
        config.core.ntfy.server.is_some() && config.core.ntfy.topic.is_some(),
        "configured",
        "optional and not configured",
    ));
    let service_detail = if verbose {
        format!("{} via {}", host.daemon.state, host.daemon.backend)
    } else {
        host.daemon.state.to_string()
    };
    checks.push(DoctorCheck::new(
        classify_service_state(&service_detail),
        "service",
        service_detail,
    ));
    if verbose {
        checks.push(DoctorCheck::new(
            CheckStatus::Ok,
            "paths",
            format!(
                "home={} config={} database={} log={} pid={}",
                paths.home.display(),
                paths.config.display(),
                paths.database.display(),
                paths.log.display(),
                paths.pid.display()
            ),
        ));
        checks.push(DoctorCheck::new(
            CheckStatus::Ok,
            "config dir",
            format!(
                "{} ({} entries)",
                host.config_dir.path.display(),
                host.config_dir.entry_count
            ),
        ));
        let port_detail = match &host.network.port_in_use {
            PortInUse::Free => "free".to_string(),
            PortInUse::InUse { pid, command } => {
                let pid_text = pid.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string());
                let command_text = command.as_deref().unwrap_or("?");
                format!("in use (pid {pid_text}, command {command_text})")
            }
            PortInUse::Unknown { reason } => format!("unknown ({reason})"),
        };
        checks.push(DoctorCheck::new(CheckStatus::Ok, "port", port_detail));
    }
    checks
}

fn classify_required(
    present: bool,
    name: &'static str,
    ok_detail: impl Into<String>,
    fail_detail: impl Into<String>,
) -> DoctorCheck {
    if present {
        DoctorCheck::new(CheckStatus::Ok, name, ok_detail)
    } else {
        DoctorCheck::new(CheckStatus::Fail, name, fail_detail)
    }
}

fn optional_check(
    name: &'static str,
    present: bool,
    ok_detail: &'static str,
    missing_detail: &'static str,
) -> DoctorCheck {
    if present {
        DoctorCheck::new(CheckStatus::Ok, name, ok_detail)
    } else {
        DoctorCheck::new(CheckStatus::Warn, name, missing_detail)
    }
}

fn classify_service_state(detail: &str) -> CheckStatus {
    if detail.starts_with("running") {
        CheckStatus::Ok
    } else {
        CheckStatus::Warn
    }
}

fn path_check(name: &'static str, path: &Path, directory: bool) -> DoctorCheck {
    let ok = if directory {
        path.exists() && path.is_dir() && writable_dir(path)
    } else if path.exists() {
        OpenOptions::new().append(true).open(path).is_ok()
    } else {
        path.parent().map(writable_dir).unwrap_or(false)
    };
    if ok {
        DoctorCheck::new(CheckStatus::Ok, name, path.display().to_string())
    } else {
        DoctorCheck::new(
            CheckStatus::Fail,
            name,
            format!("not writable: {}", path.display()),
        )
    }
}

fn database_check(paths: &RuntimePaths) -> DoctorCheck {
    let ok = paths.database.parent().map(writable_dir).unwrap_or(false);
    if ok {
        DoctorCheck::new(
            CheckStatus::Ok,
            "database",
            paths.database.display().to_string(),
        )
    } else {
        DoctorCheck::new(
            CheckStatus::Fail,
            "database",
            format!("parent not writable: {}", paths.database.display()),
        )
    }
}

fn cockpit_assets_check() -> DoctorCheck {
    #[cfg(not(debug_assertions))]
    {
        DoctorCheck::new(
            CheckStatus::Ok,
            "cockpit assets",
            "embedded in release binary",
        )
    }

    #[cfg(debug_assertions)]
    {
        let path = Path::new("cockpit/dist/index.html");
        if path.exists() {
            DoctorCheck::new(
                CheckStatus::Ok,
                "cockpit assets",
                path.display().to_string(),
            )
        } else {
            DoctorCheck::new(
                CheckStatus::Warn,
                "cockpit assets",
                "cockpit/dist/index.html not found",
            )
        }
    }
}

fn writable_dir(path: &Path) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }
    let probe = path.join(".asylum-write-check");
    match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

impl CheckStatus {
    fn marker(self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok  ",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        }
    }
}

fn run_logs(paths: &RuntimePaths, tail: bool) -> Result<()> {
    println!("Log: {}", paths.log.display());
    if tail {
        let status = ProcessCommand::new("tail")
            .arg("-f")
            .arg(&paths.log)
            .status()
            .context("run tail -f")?;
        if !status.success() {
            return Err(anyhow!("tail exited with {status}"));
        }
        return Ok(());
    }
    if !paths.log.exists() {
        println!("No log file yet.");
        return Ok(());
    }
    // Shell out to tail so large log files are not fully slurped into memory (L7).
    let status = ProcessCommand::new("tail")
        .arg("-n")
        .arg("80")
        .arg(&paths.log)
        .status()
        .context("run tail -n 80")?;
    if !status.success() {
        return Err(anyhow!("tail exited with {status}"));
    }
    Ok(())
}

// ------------------------------------------------------------------
// Uninstall
// ------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum PlanStep {
    StopDaemon { running: bool },
    RemoveServiceUnit { backend: String, path: Option<PathBuf>, installed: bool },
    RemoveBinary { path: Option<PathBuf>, present: bool },
    RemoveRuntimeDir { path: PathBuf, present: bool, preserve_logs: bool },
    KeepRuntimeDir { path: PathBuf, reason: String },
    RemoveConfigDir { path: PathBuf, present: bool },
    KeepConfigDir { path: PathBuf, reason: String },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
enum StepResult {
    Removed { path: PathBuf },
    Preserved { path: PathBuf, reason: String },
    NotPresent { path: PathBuf },
    Stopped,
    NotRunning,
    Failed { path: Option<PathBuf>, error: String },
    Noop,
}

#[derive(Clone, Debug, Serialize)]
struct UninstallReport {
    plan: Vec<PlanStep>,
    executed: bool,
    results: Vec<(String, StepResult)>,
    remaining: Option<HostState>,
}

async fn run_uninstall(
    paths: &RuntimePaths,
    client: &AsylumClient,
    args: UninstallArgs,
) -> Result<()> {
    // The daemon is running iff health responds; HostState.daemon.state is best-effort.
    let mut state = HostState::collect(paths);
    if client.is_healthy().await {
        state.daemon.healthy = true;
        state.daemon.state = ServiceState::Running;
    }

    let keep_state = args.keep.contains(&KeepKind::State);
    let keep_logs = args.keep.contains(&KeepKind::Logs);
    // `--keep config` is the default; `--purge` is the explicit opt-in to remove.
    let remove_config = args.purge && !args.keep.contains(&KeepKind::Config);

    let plan = build_uninstall_plan(&state, keep_state, keep_logs, remove_config);

    if args.json && args.dry_run {
        let report = UninstallReport {
            plan,
            executed: false,
            results: Vec::new(),
            remaining: None,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if !args.json {
        print_uninstall_plan(&plan);
    }

    if args.dry_run {
        if !args.json {
            println!();
            println!("Dry run; no changes made.");
        }
        return Ok(());
    }

    if !args.yes && !args.json && !prompt_yes_no("Proceed with uninstall? [y/N] ")? {
        println!("Aborted.");
        return Ok(());
    }

    if args.purge {
        // Typed-name confirm is non-skippable, even with --yes.
        let typed = prompt_line(&format!(
            "Type `asylum` to confirm purge of {}: ",
            config_dir_path().display()
        ))?;
        if typed.trim() != "asylum" {
            println!("Confirmation did not match; not purging config dir.");
            return Ok(());
        }
    }

    let results = execute_uninstall_plan(paths, &plan)?;
    let remaining = HostState::collect(paths);

    if args.json {
        let report = UninstallReport {
            plan,
            executed: true,
            results,
            remaining: Some(remaining),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!();
        println!("Done.");
        print_remaining_report(&remaining);
    }
    Ok(())
}

fn build_uninstall_plan(
    state: &HostState,
    keep_state: bool,
    keep_logs: bool,
    remove_config: bool,
) -> Vec<PlanStep> {
    let mut plan = Vec::new();
    plan.push(PlanStep::StopDaemon {
        running: matches!(state.daemon.state, ServiceState::Running),
    });
    plan.push(PlanStep::RemoveServiceUnit {
        backend: state.service_unit.backend.to_string(),
        path: state.service_unit.path.clone(),
        installed: state.service_unit.installed,
    });
    plan.push(PlanStep::RemoveBinary {
        path: state.binary.path.clone(),
        present: state.binary.path.as_deref().map(Path::exists).unwrap_or(false),
    });
    if keep_state {
        plan.push(PlanStep::KeepRuntimeDir {
            path: state.runtime_dir.path.clone(),
            reason: "--keep state".to_string(),
        });
    } else {
        plan.push(PlanStep::RemoveRuntimeDir {
            path: state.runtime_dir.path.clone(),
            present: state.runtime_dir.present,
            preserve_logs: keep_logs,
        });
    }
    if remove_config {
        plan.push(PlanStep::RemoveConfigDir {
            path: state.config_dir.path.clone(),
            present: state.config_dir.present,
        });
    } else {
        plan.push(PlanStep::KeepConfigDir {
            path: state.config_dir.path.clone(),
            reason: if state.config_dir.present {
                "preserved by default; pass --purge to remove (signing keys live here)".to_string()
            } else {
                "not present".to_string()
            },
        });
    }
    plan
}

fn print_uninstall_plan(plan: &[PlanStep]) {
    println!("Uninstall plan:");
    for step in plan {
        match step {
            PlanStep::StopDaemon { running } => {
                if *running {
                    println!("  - stop daemon (currently running)");
                } else {
                    println!("  - stop daemon (not running, no-op)");
                }
            }
            PlanStep::RemoveServiceUnit { backend, path, installed } => match (installed, path) {
                (true, Some(path)) => {
                    println!("  - remove service unit ({backend}): {}", path.display())
                }
                (false, Some(path)) => println!(
                    "  - service unit ({backend}) not installed at {}",
                    path.display()
                ),
                (_, None) => println!("  - service unit ({backend}): n/a"),
            },
            PlanStep::RemoveBinary { path, present } => match (present, path) {
                (true, Some(path)) => println!("  - remove binary: {}", path.display()),
                (false, Some(path)) => {
                    println!("  - binary not present at {}", path.display())
                }
                (_, None) => println!("  - binary path unknown"),
            },
            PlanStep::RemoveRuntimeDir {
                path,
                present,
                preserve_logs,
            } => {
                if !*present {
                    println!("  - runtime dir not present: {}", path.display());
                } else if *preserve_logs {
                    println!(
                        "  - remove runtime dir, preserve logs/: {}",
                        path.display()
                    );
                } else {
                    println!("  - remove runtime dir: {}", path.display());
                }
            }
            PlanStep::KeepRuntimeDir { path, reason } => {
                println!("  - keep runtime dir ({reason}): {}", path.display());
            }
            PlanStep::RemoveConfigDir { path, present } => {
                if *present {
                    println!("  - PURGE config dir: {}", path.display());
                } else {
                    println!("  - config dir not present: {}", path.display());
                }
            }
            PlanStep::KeepConfigDir { path, reason } => {
                println!("  - keep config dir ({reason}): {}", path.display());
            }
        }
    }
}

fn execute_uninstall_plan(
    paths: &RuntimePaths,
    plan: &[PlanStep],
) -> Result<Vec<(String, StepResult)>> {
    let mut results = Vec::new();

    for step in plan {
        match step {
            PlanStep::StopDaemon { running } => {
                if !*running {
                    results.push(("stop_daemon".to_string(), StepResult::NotRunning));
                    continue;
                }
                let manager = match ServiceManager::new(paths.clone()) {
                    Ok(manager) => manager,
                    Err(error) => {
                        results.push((
                            "stop_daemon".to_string(),
                            StepResult::Failed {
                                path: None,
                                error: error.to_string(),
                            },
                        ));
                        continue;
                    }
                };
                match manager.stop() {
                    Ok(()) => results.push(("stop_daemon".to_string(), StepResult::Stopped)),
                    Err(error) => results.push((
                        "stop_daemon".to_string(),
                        StepResult::Failed {
                            path: None,
                            error: error.to_string(),
                        },
                    )),
                }
            }
            PlanStep::RemoveServiceUnit {
                backend,
                path,
                installed,
            } => {
                let key = format!("remove_service_unit:{backend}");
                let Some(path) = path.clone() else {
                    results.push((key, StepResult::Noop));
                    continue;
                };
                if !*installed && !path.exists() {
                    results.push((key, StepResult::NotPresent { path }));
                    continue;
                }
                // Best-effort: also unload/disable via the platform tool.
                // ServiceManager::stop() already handled launchctl unload / systemctl stop.
                match fs::remove_file(&path) {
                    Ok(()) => results.push((key, StepResult::Removed { path })),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        results.push((key, StepResult::NotPresent { path }))
                    }
                    Err(error) => results.push((
                        key,
                        StepResult::Failed {
                            path: Some(path),
                            error: error.to_string(),
                        },
                    )),
                }
            }
            PlanStep::RemoveBinary { path, present } => {
                let Some(path) = path.clone() else {
                    results.push(("remove_binary".to_string(), StepResult::Noop));
                    continue;
                };
                if !*present && !path.exists() {
                    results.push(("remove_binary".to_string(), StepResult::NotPresent { path }));
                    continue;
                }
                // Refuse to remove an asylum binary inside a non-bin location, as a
                // courtesy guard (e.g. a developer workspace `target/release/asylum`).
                let in_bin_path = path
                    .parent()
                    .map(|p| {
                        p.ends_with("bin")
                            || p.ends_with("usr/bin")
                            || p.ends_with("local/bin")
                    })
                    .unwrap_or(false);
                if !in_bin_path {
                    results.push((
                        "remove_binary".to_string(),
                        StepResult::Preserved {
                            path: path.clone(),
                            reason: "binary not under a recognized bin dir; remove manually if intended".to_string(),
                        },
                    ));
                    continue;
                }
                match fs::remove_file(&path) {
                    Ok(()) => results.push(("remove_binary".to_string(), StepResult::Removed { path })),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        results.push(("remove_binary".to_string(), StepResult::NotPresent { path }))
                    }
                    Err(error) => results.push((
                        "remove_binary".to_string(),
                        StepResult::Failed {
                            path: Some(path),
                            error: error.to_string(),
                        },
                    )),
                }
            }
            PlanStep::RemoveRuntimeDir {
                path,
                present,
                preserve_logs,
            } => {
                if !*present {
                    results.push((
                        "remove_runtime_dir".to_string(),
                        StepResult::NotPresent { path: path.clone() },
                    ));
                    continue;
                }
                if *preserve_logs {
                    match remove_runtime_dir_preserving_logs(path, &paths.logs_dir()) {
                        Ok(()) => results.push((
                            "remove_runtime_dir".to_string(),
                            StepResult::Preserved {
                                path: path.clone(),
                                reason: "logs preserved".to_string(),
                            },
                        )),
                        Err(error) => results.push((
                            "remove_runtime_dir".to_string(),
                            StepResult::Failed {
                                path: Some(path.clone()),
                                error: error.to_string(),
                            },
                        )),
                    }
                } else {
                    match fs::remove_dir_all(path) {
                        Ok(()) => results.push((
                            "remove_runtime_dir".to_string(),
                            StepResult::Removed { path: path.clone() },
                        )),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            results.push((
                                "remove_runtime_dir".to_string(),
                                StepResult::NotPresent { path: path.clone() },
                            ))
                        }
                        Err(error) => results.push((
                            "remove_runtime_dir".to_string(),
                            StepResult::Failed {
                                path: Some(path.clone()),
                                error: error.to_string(),
                            },
                        )),
                    }
                }
            }
            PlanStep::KeepRuntimeDir { path, reason } => {
                results.push((
                    "keep_runtime_dir".to_string(),
                    StepResult::Preserved {
                        path: path.clone(),
                        reason: reason.clone(),
                    },
                ));
            }
            PlanStep::RemoveConfigDir { path, present } => {
                if !*present {
                    results.push((
                        "remove_config_dir".to_string(),
                        StepResult::NotPresent { path: path.clone() },
                    ));
                    continue;
                }
                match fs::remove_dir_all(path) {
                    Ok(()) => results.push((
                        "remove_config_dir".to_string(),
                        StepResult::Removed { path: path.clone() },
                    )),
                    Err(error) => results.push((
                        "remove_config_dir".to_string(),
                        StepResult::Failed {
                            path: Some(path.clone()),
                            error: error.to_string(),
                        },
                    )),
                }
            }
            PlanStep::KeepConfigDir { path, reason } => {
                results.push((
                    "keep_config_dir".to_string(),
                    StepResult::Preserved {
                        path: path.clone(),
                        reason: reason.clone(),
                    },
                ));
            }
        }
    }
    Ok(results)
}

fn remove_runtime_dir_preserving_logs(home: &Path, logs_dir: &Path) -> Result<()> {
    let entries = match fs::read_dir(home) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let logs_canonical = fs::canonicalize(logs_dir).ok();
    for entry in entries.flatten() {
        let path = entry.path();
        let same_as_logs = match (&logs_canonical, fs::canonicalize(&path).ok()) {
            (Some(a), Some(b)) => a == &b,
            _ => path == *logs_dir,
        };
        if same_as_logs {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("remove {}", path.display()))?;
        } else {
            fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        }
    }
    Ok(())
}

fn print_remaining_report(state: &HostState) {
    println!("Remaining:");
    println!(
        "  runtime dir present: {}",
        state.runtime_dir.present
    );
    println!("  config dir present: {}", state.config_dir.present);
    if let Some(path) = state.binary.path.as_ref() {
        if path.exists() {
            println!("  binary still present: {}", path.display());
        }
    }
    if !state.binary.shadowed_by.is_empty() {
        println!("  PATH still references other asylum binaries:");
        for path in &state.binary.shadowed_by {
            println!("    {}", path.display());
        }
    }
    if state.service_unit.installed {
        if let Some(path) = &state.service_unit.path {
            println!(
                "  service unit still installed: {}",
                path.display()
            );
        }
    }
}

fn prompt_yes_no(prompt: &str) -> Result<bool> {
    let answer = prompt_line(prompt)?;
    Ok(matches!(
        answer.trim().to_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn prompt_line(prompt: &str) -> Result<String> {
    let mut stdout = io::stdout();
    stdout.write_all(prompt.as_bytes())?;
    stdout.flush()?;
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    Ok(line)
}

async fn run_update(
    paths: &RuntimePaths,
    client: &AsylumClient,
    version: Option<String>,
) -> Result<()> {
    let current_exe = env::current_exe().context("locate asylum executable")?;
    let install_dir = current_exe
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("could not determine asylum installation directory"))?;
    let mut cleanup_installer = None;
    let installer = match update_installer_source(&current_exe) {
        UpdateInstallerSource::Local(installer_path) => installer_path,
        UpdateInstallerSource::Remote => {
            let path = download_update_installer().await?;
            cleanup_installer = Some(path.clone());
            path
        }
    };

    let manager = ServiceManager::new(paths.clone())?;
    let restart_bind = if update_needs_running_service(client.is_healthy().await, manager.status())
    {
        Some(effective_service_bind(paths)?)
    } else {
        None
    };
    if restart_bind.is_some() {
        manager.stop()?;
        println!("Asylum stopped for update");
    }

    let mut command = ProcessCommand::new("bash");
    command.args(update_installer_args(
        &installer,
        version.as_deref(),
        &install_dir,
        &paths.home,
    ));
    let status_result = command
        .status()
        .with_context(|| format!("run installer {}", installer.display()));
    if let Some(installer_tmp) = cleanup_installer {
        let _ = fs::remove_file(&installer_tmp);
    }
    let install_result = match status_result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(anyhow!("installer exited with {status}")),
        Err(error) => Err(error),
    };

    if let Err(install_error) = install_result {
        if let Some(bind) = restart_bind.as_ref() {
            if let Err(restart_error) = restart_service_after_update(&manager, client, bind).await {
                return Err(anyhow!(
                    "{install_error}; additionally failed to restart Asylum after failed update: {restart_error}"
                ));
            }
        }
        return Err(install_error);
    }

    let mut restart_error: Option<anyhow::Error> = None;
    if let Some(bind) = restart_bind.as_ref() {
        if let Err(error) = restart_service_after_update(&manager, client, bind).await {
            restart_error = Some(error);
        }
    }

    let doctor_result = run_doctor(paths, client, false).await;
    match (restart_error, doctor_result) {
        (None, result) => result,
        (Some(restart_err), Ok(())) => Err(restart_err),
        (Some(restart_err), Err(doctor_err)) => {
            // Surface both: restart failure context prepended to doctor error (L8).
            Err(anyhow!("{restart_err:#}; doctor also reported: {doctor_err:#}"))
        }
    }
}

async fn restart_service_after_update(
    manager: &ServiceManager,
    client: &AsylumClient,
    bind: &SocketAddr,
) -> Result<()> {
    manager.start(&bind.to_string())?;
    if wait_for_health(client, Duration::from_secs(6))
        .await
        .is_none()
    {
        println!(
            "Asylum update requested restart; waiting for health timed out, check status manually."
        );
    }
    Ok(())
}

fn update_needs_running_service(healthy: bool, service_state: ServiceState) -> bool {
    matches!(
        service_state_from_health(healthy, service_state),
        ServiceState::Running
    )
}

#[derive(Debug, PartialEq, Eq)]
enum UpdateInstallerSource {
    Local(PathBuf),
    Remote,
}

fn update_installer_source(executable: &Path) -> UpdateInstallerSource {
    installer_candidates_for_exe(executable)
        .into_iter()
        .find(|path| path.exists())
        .map(UpdateInstallerSource::Local)
        .unwrap_or(UpdateInstallerSource::Remote)
}

fn update_installer_args(
    installer: &Path,
    version: Option<&str>,
    install_dir: &Path,
    asylum_home: &Path,
) -> Vec<String> {
    let mut args = vec![installer.display().to_string()];
    if let Some(version) = version {
        args.push("--version".into());
        args.push(version.to_string());
    }
    args.push("--install-dir".into());
    args.push(install_dir.display().to_string());
    args.push("--asylum-home".into());
    args.push(asylum_home.display().to_string());
    args.push("--skip-setup".into());
    args.push("--skip-doctor".into());
    args
}

async fn download_update_installer() -> Result<PathBuf> {
    let response = reqwest::get(PUBLIC_INSTALLER_URL)
        .await
        .context("download update installer")?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "installer download failed with {}",
            response.status()
        ));
    }

    let body = response.bytes().await.context("read installer payload")?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("read system clock")?
        .as_nanos();
    let filename = format!("asylum-update-installer-{nanos}.sh");
    let script_path = env::temp_dir().join(filename);

    let mut file = fs::File::create(&script_path).context("create temporary installer script")?;
    file.write_all(&body)
        .context("write temporary installer script")?;
    file.flush().context("flush temporary installer script")?;
    Ok(script_path)
}

fn installer_candidates_for_exe(exe: &Path) -> Vec<PathBuf> {
    let Some(parent) = exe.parent() else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for ancestor in parent.ancestors().take(6) {
        let candidate = ancestor.join("scripts").join("install.sh");
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

#[allow(dead_code)]
fn backend_name(backend: ServiceBackend) -> &'static str {
    match backend {
        ServiceBackend::Launchd => "launchd",
        ServiceBackend::SystemdUser => "systemd user",
        ServiceBackend::PidFallback => "pid fallback",
    }
}

#[allow(dead_code)]
fn state_name(state: ServiceState) -> String {
    state.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn bare_command_defaults_to_cockpit_dispatch_shape() -> Result<()> {
        let cli = Cli::try_parse_from(["asylum"])?;
        assert!(cli.command.is_none());
        let cli = Cli::try_parse_from(["asylum", "cockpit"])?;
        assert!(matches!(cli.command, Some(Command::Cockpit)));
        Ok(())
    }

    #[test]
    fn friendly_and_advanced_commands_parse() -> Result<()> {
        let cli = Cli::try_parse_from(["asylum", "doctor", "--verbose"])?;
        assert!(matches!(
            cli.command,
            Some(Command::Doctor { verbose: true })
        ));
        let cli = Cli::try_parse_from(["asylum", "serve", "--database", "/tmp/a.db"])?;
        assert!(matches!(cli.command, Some(Command::Serve { .. })));
        let cli = Cli::try_parse_from(["asylum", "serve", "--config", "/tmp/config.toml"])?;
        assert_eq!(cli.config, Some(PathBuf::from("/tmp/config.toml")));
        let cli = Cli::try_parse_from(["asylum", "--config", "/tmp/config.toml", "status"])?;
        assert_eq!(cli.config, Some(PathBuf::from("/tmp/config.toml")));
        Ok(())
    }

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
    fn setup_runtime_is_idempotent() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        let first = setup_runtime(&paths)?;
        let second = setup_runtime(&paths)?;
        assert!(first.created_config);
        assert!(!second.created_config);
        assert!(paths.config.exists());
        assert!(paths.logs_dir().exists());
        assert!(paths.run_dir().exists());
        Ok(())
    }

    #[test]
    fn service_bind_uses_config_listen() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        let mut file = ConfigFile::default_for_paths(&paths);
        file.core.listen = Some("127.0.0.1:9011".to_string());
        paths.ensure_dirs()?;
        fs::write(
            &paths.config,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        env::remove_var("ASYLUM_BIND");

        let bind = effective_service_bind(&paths)?;
        assert_eq!(bind.to_string(), "127.0.0.1:9011");

        if let Some(value) = prev_bind {
            env::set_var("ASYLUM_BIND", value);
        }
        Ok(())
    }

    #[test]
    fn service_bind_uses_env_before_config_listen() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        let mut file = ConfigFile::default_for_paths(&paths);
        file.core.listen = Some("127.0.0.1:9011".to_string());
        paths.ensure_dirs()?;
        fs::write(
            &paths.config,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        env::set_var("ASYLUM_BIND", "127.0.0.1:9012");

        let service_bind = effective_service_bind(&paths)?;
        let serve_bind = load_serve_config(empty_serve_config(), &paths)?.bind;
        assert_eq!(service_bind.to_string(), "127.0.0.1:9012");
        assert_eq!(serve_bind, service_bind);

        if let Some(value) = prev_bind {
            env::set_var("ASYLUM_BIND", value);
        } else {
            env::remove_var("ASYLUM_BIND");
        }
        Ok(())
    }

    #[test]
    fn runtime_client_uses_config_bind_when_base_url_is_unset() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        write_config_with_listen(&paths, "127.0.0.1:9021")?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::remove_var("ASYLUM_BIND");
        env::remove_var("ASYLUM_BASE_URL");

        let client = runtime_client(&paths)?;
        assert_eq!(client.base_url(), "http://127.0.0.1:9021");

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn runtime_client_uses_env_bind_when_base_url_is_unset() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        write_config_with_listen(&paths, "127.0.0.1:9021")?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::set_var("ASYLUM_BIND", "127.0.0.1:9022");
        env::remove_var("ASYLUM_BASE_URL");

        let client = runtime_client(&paths)?;
        assert_eq!(client.base_url(), "http://127.0.0.1:9022");

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn runtime_client_honors_base_url_override() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        write_config_with_listen(&paths, "127.0.0.1:9021")?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::set_var("ASYLUM_BIND", "127.0.0.1:9022");
        env::set_var("ASYLUM_BASE_URL", "http://example.test:9900");

        let client = runtime_client(&paths)?;
        assert_eq!(client.base_url(), "http://example.test:9900");

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn data_plane_arms_use_runtime_client_url_from_config() -> Result<()> {
        // Verify that runtime_client (used by all dispatch arms after H3 fix)
        // resolves the base URL from config `listen`, not the hard-coded default.
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        write_config_with_listen(&paths, "127.0.0.1:9999")?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::remove_var("ASYLUM_BIND");
        env::remove_var("ASYLUM_BASE_URL");

        let client = runtime_client(&paths)?;
        // The data-plane client must NOT point at the hard-coded default (7717).
        assert_ne!(
            client.base_url(),
            "http://127.0.0.1:7717",
            "data-plane client must not use hard-coded default when config overrides listen"
        );
        assert_eq!(
            client.base_url(),
            "http://127.0.0.1:9999",
            "data-plane client must use config listen address"
        );

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn serve_config_base_url_tracks_effective_bind_without_override() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        write_config_with_listen(&paths, "127.0.0.1:9031")?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::set_var("ASYLUM_BIND", "127.0.0.1:9032");
        env::remove_var("ASYLUM_BASE_URL");

        let state = load_serve_config(empty_serve_config(), &paths)?;
        assert_eq!(state.bind.to_string(), "127.0.0.1:9032");
        assert_eq!(state.config.base_url, "http://127.0.0.1:9032");

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn serve_config_base_url_honors_env_override() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        write_config_with_listen(&paths, "127.0.0.1:9031")?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::set_var("ASYLUM_BIND", "127.0.0.1:9032");
        env::set_var("ASYLUM_BASE_URL", "http://public.example");

        let state = load_serve_config(empty_serve_config(), &paths)?;
        assert_eq!(state.bind.to_string(), "127.0.0.1:9032");
        assert_eq!(state.config.base_url, "http://public.example");

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn serve_config_preserves_explicit_file_base_url() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        let mut file = ConfigFile::default_for_paths(&paths);
        file.core.listen = Some("127.0.0.1:9041".to_string());
        file.core.base_url = "https://public.example".to_string();
        paths.ensure_dirs()?;
        fs::write(
            &paths.config,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::remove_var("ASYLUM_BIND");
        env::remove_var("ASYLUM_BASE_URL");

        let state = load_serve_config(empty_serve_config(), &paths)?;
        assert_eq!(state.bind.to_string(), "127.0.0.1:9041");
        assert_eq!(state.config.base_url, "https://public.example");

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn serve_config_aligns_default_file_base_url_to_changed_listen() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        let mut file = ConfigFile::default_for_paths(&paths);
        file.core.listen = Some("127.0.0.1:9042".to_string());
        file.core.base_url = DEFAULT_BASE_URL.to_string();
        paths.ensure_dirs()?;
        fs::write(
            &paths.config,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::remove_var("ASYLUM_BIND");
        env::remove_var("ASYLUM_BASE_URL");

        let state = load_serve_config(empty_serve_config(), &paths)?;
        assert_eq!(state.bind.to_string(), "127.0.0.1:9042");
        assert_eq!(state.config.base_url, "http://127.0.0.1:9042");

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn installer_candidates_are_executable_relative() {
        let candidates = installer_candidates_for_exe(Path::new("/repo/target/debug/asylum"));
        assert_eq!(
            candidates[0],
            PathBuf::from("/repo/target/debug/scripts/install.sh")
        );
        assert_eq!(
            candidates[1],
            PathBuf::from("/repo/target/scripts/install.sh")
        );
        assert_eq!(candidates[2], PathBuf::from("/repo/scripts/install.sh"));
        assert!(!candidates.contains(&PathBuf::from("scripts/install.sh")));
    }

    #[test]
    fn update_installer_source_prefers_local_installer() -> Result<()> {
        let root = tempfile::tempdir()?;
        let exe = root.path().join("repo/target/debug/asylum");
        std::fs::create_dir_all(exe.parent().unwrap())?;
        std::fs::File::create(&exe)?;

        let local_installer = exe.parent().unwrap().join("scripts").join("install.sh");
        std::fs::create_dir_all(local_installer.parent().unwrap())?;
        std::fs::File::create(&local_installer)?;

        assert_eq!(
            update_installer_source(&exe),
            UpdateInstallerSource::Local(local_installer)
        );
        Ok(())
    }

    #[test]
    fn update_installer_source_falls_back_to_remote() -> Result<()> {
        let root = tempfile::tempdir()?;
        let exe = root.path().join("asylum");
        std::fs::create_dir_all(exe.parent().unwrap())?;
        std::fs::File::create(&exe)?;

        assert_eq!(update_installer_source(&exe), UpdateInstallerSource::Remote);
        Ok(())
    }

    #[test]
    fn update_installer_args_are_update_shape() {
        let args = update_installer_args(
            Path::new("/tmp/scripts/install.sh"),
            Some("v2.0.0"),
            Path::new("/opt/asylum/bin"),
            Path::new("/Users/test/.asylum"),
        );
        assert_eq!(
            args,
            vec![
                "/tmp/scripts/install.sh",
                "--version",
                "v2.0.0",
                "--install-dir",
                "/opt/asylum/bin",
                "--asylum-home",
                "/Users/test/.asylum",
                "--skip-setup",
                "--skip-doctor"
            ]
        );
    }

    #[test]
    fn update_needs_running_service_detects_running_planes() {
        assert!(update_needs_running_service(true, ServiceState::Stopped));
        assert!(update_needs_running_service(true, ServiceState::Running));
        assert!(update_needs_running_service(false, ServiceState::Running));
        assert!(!update_needs_running_service(
            false,
            ServiceState::Unknown("launchd".to_string())
        ));
    }

    #[test]
    fn cockpit_only_opens_after_health_ready_start() {
        assert!(StartOutcome::AlreadyHealthy.is_health_ready());
        assert!(StartOutcome::StartedHealthy.is_health_ready());
        assert!(!StartOutcome::StartRequestedHealthTimedOut.is_health_ready());
    }

    #[test]
    fn doctor_classifies_required_and_optional_checks() {
        let required = classify_required(true, "tool", "present", "missing");
        assert_eq!(required.status, CheckStatus::Ok);
        let required = classify_required(false, "tool", "present", "missing");
        assert_eq!(required.status, CheckStatus::Fail);
        let optional = optional_check("ntfy", false, "configured", "optional");
        assert_eq!(optional.status, CheckStatus::Warn);
        assert_eq!(classify_service_state("running via pid"), CheckStatus::Ok);
        assert_eq!(classify_service_state("stopped"), CheckStatus::Warn);
    }

    #[test]
    fn serve_config_merges_cli_overrides_with_file() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        let mut file = ConfigFile::default_for_paths(&paths);
        file.core.base_url = "http://from-file".to_string();
        file.core.workspace.recent_limit = 5;
        file.database = ".asylum/asylum.sqlite3".to_string();
        file.core.ntfy.server = Some("https://from-file".to_string());
        paths.ensure_dirs()?;
        fs::write(
            &paths.config,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;

        let args = ServeConfig {
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
        } = load_serve_config(args, &paths)?;

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
        let paths = RuntimePaths::from_values(Some(tempdir.path().to_path_buf()), None, None, None);
        let file = ConfigFile::default_for_paths(&paths);
        paths.ensure_dirs()?;
        fs::write(
            &paths.config,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;

        let prev_token = env::var_os("ASYLUM_OWNER_TOKEN");
        let prev_enabled = env::var_os("ASYLUM_OWNER_TOKENS_ENABLED");
        env::set_var("ASYLUM_OWNER_TOKEN", "env-owner");
        env::set_var("ASYLUM_OWNER_TOKENS_ENABLED", "true");

        let args = ServeConfig {
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

        let ServeState { config, .. } = load_serve_config(args, &paths)?;
        assert_eq!(config.auth.owner_token.as_deref(), Some("env-owner"));
        assert!(config.auth.owner_tokens_enabled);

        if let Some(value) = prev_token {
            env::set_var("ASYLUM_OWNER_TOKEN", value);
        } else {
            env::remove_var("ASYLUM_OWNER_TOKEN");
        }
        if let Some(value) = prev_enabled {
            env::set_var("ASYLUM_OWNER_TOKENS_ENABLED", value);
        } else {
            env::remove_var("ASYLUM_OWNER_TOKENS_ENABLED");
        }
        Ok(())
    }

    fn empty_serve_config() -> ServeConfig {
        ServeConfig {
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
        }
    }

    fn write_config_with_listen(paths: &RuntimePaths, listen: &str) -> Result<()> {
        let mut file = ConfigFile::default_for_paths(paths);
        file.core.listen = Some(listen.to_string());
        paths.ensure_dirs()?;
        fs::write(
            &paths.config,
            toml::to_string_pretty(&file).context("serialize config file")?,
        )?;
        Ok(())
    }

    fn restore_env(name: &str, previous: Option<std::ffi::OsString>) {
        if let Some(value) = previous {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }
}
