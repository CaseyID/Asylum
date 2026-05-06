pub mod app;
pub mod attach;
pub mod auth;
pub mod capability_service;
pub mod channels;
pub mod decision_ingester;
pub mod harness;
pub mod hooks;
pub mod notifications;
pub mod recipes;
pub mod remote_commands;
pub mod storage;
pub mod substrate;

use std::env;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use asylum_types::config::{AsylumConfig, AsylumFileConfig, NtfyConfig};

const DEFAULT_BIND: &str = "127.0.0.1:7717";
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:7717";

#[derive(Clone, Debug, Default)]
pub struct DaemonRunOptions {
    pub config: Option<PathBuf>,
    pub bind: Option<SocketAddr>,
    pub database: Option<String>,
    pub socket_path: Option<PathBuf>,
    pub base_url: Option<String>,
    pub owner_token: Option<String>,
    pub owner_tokens_enabled: bool,
    pub ntfy_server: Option<String>,
    pub ntfy_topic: Option<String>,
    pub ntfy_token: Option<String>,
    pub loon_enabled: bool,
    pub loon_endpoint: Option<String>,
    pub loon_cli_path: Option<PathBuf>,
    pub harness_codex_command: Option<String>,
    pub harness_claude_command: Option<String>,
    pub workspace_recent_limit: Option<usize>,
}

struct DaemonRuntimePaths {
    config: PathBuf,
    database: PathBuf,
    socket: PathBuf,
}

struct DaemonRunState {
    bind: SocketAddr,
    database: String,
    socket_path: PathBuf,
    config: AsylumConfig,
}

pub async fn run(options: DaemonRunOptions) -> Result<()> {
    let state = load_run_state(options)?;
    app::serve_with_socket(
        state.bind,
        state.database,
        Some(state.socket_path),
        state.config,
    )
    .await
}

fn load_run_state(options: DaemonRunOptions) -> Result<DaemonRunState> {
    let paths = DaemonRuntimePaths::from_env(options.config.clone(), options.database.clone());
    let file_config = load_config_file(&paths)?;
    let mut config = file_config.core;
    let base_url_from_file = config.base_url.clone();
    let base_url_overridden = options.base_url.is_some()
        || env::var_os("ASYLUM_BASE_URL").is_some()
        || is_explicit_config_base_url(&base_url_from_file);

    apply_bind_env_override(&mut config);
    apply_env_overrides(&mut config);
    apply_cli_overrides(&mut config, &options);

    let bind = effective_bind(options.bind, &config);
    if !base_url_overridden {
        config.base_url = local_base_url_for_bind(bind);
    }

    let database = options
        .database
        .or_else(|| env::var("ASYLUM_DATABASE").ok())
        .unwrap_or(file_config.database);
    let socket_path = options
        .socket_path
        .or_else(|| env::var_os("ASYLUM_SOCKET_PATH").map(PathBuf::from))
        .unwrap_or(paths.socket);

    ensure_daemon_paths(&database, &socket_path)?;

    Ok(DaemonRunState {
        bind,
        database,
        socket_path,
        config,
    })
}

impl DaemonRuntimePaths {
    fn from_env(config_override: Option<PathBuf>, database_override: Option<String>) -> Self {
        let home = env::var_os("ASYLUM_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".asylum")
            });
        let config = config_override
            .or_else(|| env::var_os("ASYLUM_CONFIG").map(PathBuf::from))
            .unwrap_or_else(|| home.join("config.toml"));
        let database = database_override
            .map(PathBuf::from)
            .or_else(|| env::var_os("ASYLUM_DATABASE").map(PathBuf::from))
            .unwrap_or_else(|| home.join("asylum.sqlite3"));
        let socket = env::var_os("ASYLUM_SOCKET_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("run").join("asylum.sock"));

        Self {
            config,
            database,
            socket,
        }
    }
}

fn default_file_config_for_paths(paths: &DaemonRuntimePaths) -> AsylumFileConfig {
    let mut config = AsylumConfig::default();
    config.listen = Some(DEFAULT_BIND.to_string());
    config.base_url = DEFAULT_BASE_URL.to_string();
    config.ntfy = NtfyConfig {
        server: None,
        topic: None,
        token: None,
        poll_interval_seconds: 30,
    };
    config.harness.codex_command = "codex".to_string();
    config.harness.claude_command = "claude".to_string();

    AsylumFileConfig {
        core: config,
        database: paths.database.display().to_string(),
    }
}

fn load_config_file(paths: &DaemonRuntimePaths) -> Result<AsylumFileConfig> {
    if !paths.config.exists() {
        return Ok(default_file_config_for_paths(paths));
    }

    let content = std::fs::read_to_string(&paths.config)
        .with_context(|| format!("read config file {}", paths.config.display()))?;
    toml::from_str::<AsylumFileConfig>(&content)
        .with_context(|| format!("parse config file {}", paths.config.display()))
}

fn ensure_daemon_paths(database: &str, socket_path: &Path) -> Result<()> {
    if let Some(parent) = Path::new(database).parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create database directory {}", parent.display()))?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create socket directory {}", parent.display()))?;
    }
    Ok(())
}

fn parse_bool_flag(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
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

fn apply_cli_overrides(config: &mut AsylumConfig, args: &DaemonRunOptions) {
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

fn is_explicit_config_base_url(value: &str) -> bool {
    !value.is_empty() && value != DEFAULT_BASE_URL
}

fn local_base_url_for_bind(bind: SocketAddr) -> String {
    let ip = if bind.ip().is_unspecified() {
        if bind.is_ipv6() {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        }
    } else {
        bind.ip()
    };
    format!("http://{}", SocketAddr::new(ip, bind.port()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn run_state_aligns_default_base_url_to_effective_bind() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let config = tempdir.path().join("config.toml");
        let database = tempdir.path().join("asylum.sqlite3");
        let mut file = default_file_config_for_test(database.clone());
        file.core.listen = Some("127.0.0.1:9042".to_string());
        file.core.base_url = DEFAULT_BASE_URL.to_string();
        std::fs::write(&config, toml::to_string_pretty(&file)?)?;

        let prev_bind = env::var_os("ASYLUM_BIND");
        let prev_base_url = env::var_os("ASYLUM_BASE_URL");
        env::remove_var("ASYLUM_BIND");
        env::remove_var("ASYLUM_BASE_URL");

        let state = load_run_state(DaemonRunOptions {
            config: Some(config),
            ..DaemonRunOptions::default()
        })?;

        assert_eq!(state.bind.to_string(), "127.0.0.1:9042");
        assert_eq!(state.config.base_url, "http://127.0.0.1:9042");

        restore_env("ASYLUM_BIND", prev_bind);
        restore_env("ASYLUM_BASE_URL", prev_base_url);
        Ok(())
    }

    #[test]
    fn run_state_merges_cli_and_env_overrides_with_file_config() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let config = tempdir.path().join("config.toml");
        let database = tempdir.path().join("asylum.sqlite3");
        let mut file = default_file_config_for_test(database.clone());
        file.core.base_url = "http://from-file".to_string();
        file.core.workspace.recent_limit = 5;
        file.core.ntfy.server = Some("https://from-file".to_string());
        file.database = ".asylum/asylum.sqlite3".to_string();
        std::fs::write(&config, toml::to_string_pretty(&file)?)?;

        let prev_token = env::var_os("ASYLUM_OWNER_TOKEN");
        let prev_enabled = env::var_os("ASYLUM_OWNER_TOKENS_ENABLED");
        env::set_var("ASYLUM_OWNER_TOKEN", "env-owner");
        env::set_var("ASYLUM_OWNER_TOKENS_ENABLED", "true");

        let state = load_run_state(DaemonRunOptions {
            config: Some(config),
            bind: Some("127.0.0.1:9000".parse()?),
            base_url: Some("http://from-cli".to_string()),
            ntfy_server: Some("https://from-cli".to_string()),
            loon_enabled: true,
            loon_endpoint: Some("http://loon".to_string()),
            harness_claude_command: Some("claude-cli".to_string()),
            workspace_recent_limit: Some(11),
            ..DaemonRunOptions::default()
        })?;

        assert_eq!(state.bind.to_string(), "127.0.0.1:9000");
        assert_eq!(state.database, ".asylum/asylum.sqlite3");
        assert_eq!(state.config.base_url, "http://from-cli");
        assert_eq!(
            state.config.ntfy.server,
            Some("https://from-cli".to_string())
        );
        assert_eq!(state.config.auth.owner_token.as_deref(), Some("env-owner"));
        assert!(state.config.auth.owner_tokens_enabled);
        assert_eq!(state.config.workspace.recent_limit, 11);
        assert_eq!(state.config.harness.claude_command, "claude-cli");
        assert_eq!(state.config.loon.endpoint, "http://loon");
        assert!(state.config.loon.enabled);

        restore_env("ASYLUM_OWNER_TOKEN", prev_token);
        restore_env("ASYLUM_OWNER_TOKENS_ENABLED", prev_enabled);
        Ok(())
    }

    #[test]
    fn run_state_honors_socket_path_override() -> Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let config = tempdir.path().join("config.toml");
        let database = tempdir.path().join("asylum.sqlite3");
        let file = default_file_config_for_test(database);
        std::fs::write(&config, toml::to_string_pretty(&file)?)?;

        let socket_path = tempdir.path().join("custom.sock");
        let state = load_run_state(DaemonRunOptions {
            config: Some(config),
            socket_path: Some(socket_path.clone()),
            ..DaemonRunOptions::default()
        })?;

        assert_eq!(state.socket_path, socket_path);
        Ok(())
    }

    fn default_file_config_for_test(database: PathBuf) -> AsylumFileConfig {
        let paths = DaemonRuntimePaths {
            config: PathBuf::from("config.toml"),
            database,
            socket: PathBuf::from("asylum.sock"),
        };
        default_file_config_for_paths(&paths)
    }

    fn restore_env(name: &str, previous: Option<std::ffi::OsString>) {
        if let Some(value) = previous {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }
}
