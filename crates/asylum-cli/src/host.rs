use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt as _;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::runtime::RuntimePaths;

const LABEL: &str = "dev.asylum.daemon";

// ------------------------------------------------------------------
// Service backend / state (folded in from former `service.rs`)
// ------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceBackend {
    Launchd,
    SystemdUser,
    PidFallback,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ServiceState {
    Running,
    Stopped,
    Unknown(String),
}

#[derive(Clone, Debug)]
pub struct ServiceRenderConfig {
    pub binary: PathBuf,
    pub config: PathBuf,
    pub log: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ServiceManager {
    backend: ServiceBackend,
    paths: RuntimePaths,
    binary: PathBuf,
}

impl ServiceManager {
    pub fn new(paths: RuntimePaths) -> Result<Self> {
        let binary = std::env::current_exe().context("locate asylum executable")?;
        Ok(Self::with_backend(paths, binary, select_backend()))
    }

    pub fn with_backend(paths: RuntimePaths, binary: PathBuf, backend: ServiceBackend) -> Self {
        Self {
            backend,
            paths,
            binary,
        }
    }

    pub fn backend(&self) -> ServiceBackend {
        self.backend
    }

    pub fn start(&self, bind: &str) -> Result<()> {
        self.paths.ensure_dirs()?;
        match self.backend {
            ServiceBackend::Launchd => self.start_launchd(bind),
            ServiceBackend::SystemdUser => self.start_systemd(bind),
            ServiceBackend::PidFallback => self.start_pid_fallback(bind),
        }
    }

    pub fn stop(&self) -> Result<()> {
        match self.backend {
            ServiceBackend::Launchd => {
                let plist = self.launchd_plist_path();
                let _ = ProcessCommand::new("launchctl")
                    .arg("unload")
                    .arg(&plist)
                    .status();
                self.stop_pid_fallback()
            }
            ServiceBackend::SystemdUser => {
                let _ = ProcessCommand::new("systemctl")
                    .arg("--user")
                    .arg("stop")
                    .arg("asylum.service")
                    .status();
                self.stop_pid_fallback()
            }
            ServiceBackend::PidFallback => self.stop_pid_fallback(),
        }
    }

    pub fn restart(&self, bind: &str) -> Result<()> {
        self.stop()?;
        self.start(bind)
    }

    pub fn status(&self) -> ServiceState {
        if let Some(pid) = self.read_pid() {
            if process_is_running(pid) && self.pid_identity(pid) == PidIdentity::Matches {
                return ServiceState::Running;
            }
            return ServiceState::Stopped;
        }
        match self.backend {
            ServiceBackend::Launchd => command_status("launchctl", &["list", LABEL]),
            ServiceBackend::SystemdUser => {
                command_status("systemctl", &["--user", "is-active", "asylum.service"])
            }
            ServiceBackend::PidFallback => ServiceState::Stopped,
        }
    }

    pub fn render_config(&self, _bind: &str) -> ServiceRenderConfig {
        ServiceRenderConfig {
            binary: self.binary.clone(),
            config: self.paths.config.clone(),
            log: self.paths.log.clone(),
        }
    }

    pub fn launchd_plist_text(&self, bind: &str) -> String {
        render_launchd_plist(&self.render_config(bind))
    }

    pub fn systemd_unit_text(&self, bind: &str) -> String {
        render_systemd_unit(&self.render_config(bind))
    }

    pub fn service_unit_installed(&self) -> bool {
        match self.backend {
            ServiceBackend::Launchd => self.launchd_plist_path().exists(),
            ServiceBackend::SystemdUser => self.systemd_unit_path().exists(),
            ServiceBackend::PidFallback => false,
        }
    }

    pub fn refresh_installed_unit(&self, bind: &str) -> Result<bool> {
        let refreshed = self.refresh_installed_unit_file(bind)?.is_some();
        if refreshed {
            self.reload_service_definitions();
        }
        Ok(refreshed)
    }

    pub fn read_running_pid(&self) -> Option<u32> {
        let pid = self.read_pid()?;
        if process_is_running(pid) && self.pid_identity(pid) == PidIdentity::Matches {
            Some(pid)
        } else {
            None
        }
    }

    pub fn launchd_plist_location(&self) -> PathBuf {
        self.launchd_plist_path()
    }

    pub fn systemd_unit_location(&self) -> PathBuf {
        self.systemd_unit_path()
    }

    fn start_launchd(&self, bind: &str) -> Result<()> {
        let plist = self.launchd_plist_path();
        if let Some(parent) = plist.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&plist, self.launchd_plist_text(bind))?;
        let _ = ProcessCommand::new("launchctl")
            .arg("unload")
            .arg(&plist)
            .status();
        let status = ProcessCommand::new("launchctl")
            .arg("load")
            .arg(&plist)
            .status()
            .context("run launchctl load")?;
        if status.success() {
            Ok(())
        } else {
            self.start_pid_fallback(bind)
        }
    }

    fn start_systemd(&self, bind: &str) -> Result<()> {
        let unit = self.systemd_unit_path();
        if let Some(parent) = unit.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&unit, self.systemd_unit_text(bind))?;
        let _ = ProcessCommand::new("systemctl")
            .arg("--user")
            .arg("daemon-reload")
            .status();
        let status = ProcessCommand::new("systemctl")
            .arg("--user")
            .arg("start")
            .arg("asylum.service")
            .status()
            .context("run systemctl --user start")?;
        if status.success() {
            Ok(())
        } else {
            self.start_pid_fallback(bind)
        }
    }

    fn refresh_installed_unit_file(&self, bind: &str) -> Result<Option<PathBuf>> {
        match self.backend {
            ServiceBackend::Launchd => {
                let plist = self.launchd_plist_path();
                if !plist.exists() {
                    return Ok(None);
                }
                fs::write(&plist, self.launchd_plist_text(bind))
                    .with_context(|| format!("refresh launchd plist {}", plist.display()))?;
                Ok(Some(plist))
            }
            ServiceBackend::SystemdUser => {
                let unit = self.systemd_unit_path();
                if !unit.exists() {
                    return Ok(None);
                }
                fs::write(&unit, self.systemd_unit_text(bind))
                    .with_context(|| format!("refresh systemd unit {}", unit.display()))?;
                Ok(Some(unit))
            }
            ServiceBackend::PidFallback => Ok(None),
        }
    }

    fn reload_service_definitions(&self) {
        if self.backend == ServiceBackend::SystemdUser {
            let _ = ProcessCommand::new("systemctl")
                .arg("--user")
                .arg("daemon-reload")
                .status();
        }
    }

    fn start_pid_fallback(&self, bind: &str) -> Result<()> {
        if let Some(pid) = self.read_pid() {
            if process_is_running(pid) && self.pid_identity(pid) == PidIdentity::Matches {
                return Ok(());
            }
            self.remove_pid_files();
        }
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.paths.log)
            .with_context(|| format!("open log {}", self.paths.log.display()))?;
        let err_log = log.try_clone()?;
        let mut cmd = ProcessCommand::new(&self.binary);
        cmd.arg("daemon")
            .arg("run")
            .arg("--config")
            .arg(&self.paths.config)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err_log));
        // Detach from the controlling terminal so the daemon survives shell exit.
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let child = cmd
            .spawn()
            .with_context(|| format!("start {}", self.binary.display()))?;
        fs::write(&self.paths.pid, child.id().to_string())?;
        fs::write(
            self.pid_metadata_path(),
            self.pid_metadata(child.id(), bind),
        )?;
        Ok(())
    }

    fn stop_pid_fallback(&self) -> Result<()> {
        let Some(pid) = self.read_pid() else {
            return Ok(());
        };
        if process_is_running(pid) && self.pid_identity(pid) == PidIdentity::Matches {
            let _ = ProcessCommand::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .status();
        }
        self.remove_pid_files();
        Ok(())
    }

    fn read_pid(&self) -> Option<u32> {
        fs::read_to_string(&self.paths.pid)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
    }

    fn pid_identity(&self, pid: u32) -> PidIdentity {
        classify_pid_identity(
            &self.binary,
            pid,
            fs::read_to_string(self.pid_metadata_path()).ok().as_deref(),
            process_argv(pid).as_deref(),
        )
    }

    fn pid_metadata(&self, pid: u32, bind: &str) -> String {
        format!(
            "pid={pid}\nbinary={}\ncommand=daemon run\nconfig={}\ndatabase={}\nbind={bind}\n",
            self.binary.display(),
            self.paths.config.display(),
            self.paths.database.display(),
        )
    }

    fn pid_metadata_path(&self) -> PathBuf {
        metadata_path_for_pidfile(&self.paths.pid)
    }

    fn remove_pid_files(&self) {
        let _ = fs::remove_file(&self.paths.pid);
        let _ = fs::remove_file(self.pid_metadata_path());
    }

    fn launchd_plist_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| self.paths.home.clone())
            .join("Library")
            .join("LaunchAgents")
            .join("dev.asylum.daemon.plist")
    }

    fn systemd_unit_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| self.paths.home.clone())
            .join(".config")
            .join("systemd")
            .join("user")
            .join("asylum.service")
    }
}

pub fn select_backend() -> ServiceBackend {
    if cfg!(target_os = "macos") && command_exists("launchctl") {
        ServiceBackend::Launchd
    } else if cfg!(target_os = "linux") && command_exists("systemctl") {
        ServiceBackend::SystemdUser
    } else {
        ServiceBackend::PidFallback
    }
}

pub fn render_launchd_plist(config: &ServiceRenderConfig) -> String {
    let binary = xml_escape(&config.binary.display().to_string());
    let config_path = xml_escape(&config.config.display().to_string());
    let log = xml_escape(&config.log.display().to_string());
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
            "<plist version=\"1.0\">\n",
            "  <dict>\n",
            "    <key>Label</key>\n",
            "    <string>{label}</string>\n",
            "    <key>ProgramArguments</key>\n",
            "    <array>\n",
            "      <string>{binary}</string>\n",
            "      <string>daemon</string>\n",
            "      <string>run</string>\n",
            "      <string>--config</string>\n",
            "      <string>{config_path}</string>\n",
            "    </array>\n",
            "    <key>StandardOutPath</key>\n",
            "    <string>{log}</string>\n",
            "    <key>StandardErrorPath</key>\n",
            "    <string>{log}</string>\n",
            "    <key>RunAtLoad</key>\n",
            "    <true/>\n",
            "    <key>KeepAlive</key>\n",
            "    <true/>\n",
            "  </dict>\n",
            "</plist>\n",
        ),
        label = LABEL,
        binary = binary,
        config_path = config_path,
        log = log,
    )
}

pub fn render_systemd_unit(config: &ServiceRenderConfig) -> String {
    format!(
        concat!(
            "[Unit]\n",
            "Description=Asylum Control Plane\n",
            "After=network-online.target\n\n",
            "[Service]\n",
            "Type=simple\n",
            "ExecStart={} daemon run --config {}\n",
            "Restart=on-failure\n",
            "RestartSec=3\n",
            "StandardOutput=append:{}\n",
            "StandardError=append:{}\n\n",
            "[Install]\n",
            "WantedBy=default.target\n",
        ),
        systemd_quote_arg(&config.binary.display().to_string()),
        systemd_quote_arg(&config.config.display().to_string()),
        systemd_setting_path(&config.log.display().to_string()),
        systemd_setting_path(&config.log.display().to_string()),
    )
}

pub fn command_exists(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        return PathBuf::from(command).is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(command).is_file())
}

fn process_is_running(pid: u32) -> bool {
    ProcessCommand::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PidIdentity {
    Matches,
    Mismatch,
    Unknown,
}

fn pid_metadata_matches(binary: &Path, pid: u32, content: &str) -> bool {
    let mut metadata_pid = None;
    let mut metadata_binary = None;
    let mut metadata_command = None;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("pid=") {
            metadata_pid = value.parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("binary=") {
            metadata_binary = Some(value);
        } else if let Some(value) = line.strip_prefix("command=") {
            metadata_command = Some(value);
        }
    }
    let binary = binary.display().to_string();
    metadata_pid == Some(pid)
        && metadata_binary == Some(binary.as_str())
        && metadata_command == Some("daemon run")
}

fn classify_pid_identity(
    binary: &Path,
    pid: u32,
    metadata: Option<&str>,
    argv: Option<&[String]>,
) -> PidIdentity {
    let metadata_match = metadata.is_some_and(|content| pid_metadata_matches(binary, pid, content));

    if let Some(argv) = argv {
        if !command_argv_matches_asylum(binary, argv) {
            return PidIdentity::Mismatch;
        }
        return match metadata {
            Some(_) if metadata_match => PidIdentity::Matches,
            Some(_) => PidIdentity::Mismatch,
            None => PidIdentity::Unknown,
        };
    }

    if metadata.is_none() {
        PidIdentity::Unknown
    } else if metadata_match {
        PidIdentity::Matches
    } else {
        PidIdentity::Mismatch
    }
}

fn metadata_path_for_pidfile(pidfile: &Path) -> PathBuf {
    let file_name = pidfile
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| format!("{value}.meta"))
        .unwrap_or_else(|| "asylum.pid.meta".to_string());
    pidfile.with_file_name(file_name)
}

fn process_argv(pid: u32) -> Option<Vec<String>> {
    #[cfg(target_os = "linux")]
    {
        let content = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        if content.is_empty() {
            return None;
        }
        let argv = content
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).to_string())
            .collect::<Vec<_>>();
        if argv.is_empty() {
            None
        } else {
            Some(argv)
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

fn command_argv_matches_asylum(binary: &Path, argv: &[String]) -> bool {
    let binary = binary.display().to_string();
    let Some(argv0) = argv.first() else {
        return false;
    };
    let executable_matches = argv0 == &binary
        || (Path::new(argv0).file_name() == binary_path_basename(binary.as_str())
            && !argv0.contains(std::path::MAIN_SEPARATOR));
    executable_matches
        && argv.iter().any(|part| part == "daemon")
        && argv.iter().any(|part| part == "run")
}

fn binary_path_basename(binary: &str) -> Option<&std::ffi::OsStr> {
    Path::new(binary).file_name()
}

fn command_status(command: &str, args: &[&str]) -> ServiceState {
    match ProcessCommand::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => ServiceState::Running,
        Ok(_) => ServiceState::Stopped,
        Err(error) => ServiceState::Unknown(error.to_string()),
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn systemd_quote_arg(value: &str) -> String {
    let escaped = value.chars().fold(String::new(), |mut output, character| {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '%' => output.push_str("%%"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\x{:02x}", character as u32));
            }
            character => output.push(character),
        }
        output
    });
    format!("\"{escaped}\"")
}

fn systemd_setting_path(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '%' => "%%".chars().collect::<Vec<_>>(),
            ' ' => "\\x20".chars().collect::<Vec<_>>(),
            '\t' => "\\x09".chars().collect::<Vec<_>>(),
            '\n' => "\\x0a".chars().collect::<Vec<_>>(),
            '\\' => "\\x5c".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

impl std::fmt::Display for ServiceBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceBackend::Launchd => formatter.write_str("launchd"),
            ServiceBackend::SystemdUser => formatter.write_str("systemd user"),
            ServiceBackend::PidFallback => formatter.write_str("pid fallback"),
        }
    }
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceState::Running => formatter.write_str("running"),
            ServiceState::Stopped => formatter.write_str("stopped"),
            ServiceState::Unknown(message) => write!(formatter, "unknown: {message}"),
        }
    }
}

pub fn service_state_from_health(healthy: bool, service_state: ServiceState) -> ServiceState {
    if healthy {
        ServiceState::Running
    } else {
        service_state
    }
}

pub fn require_binary() -> Result<PathBuf> {
    std::env::current_exe().map_err(|error| anyhow!("locate asylum executable: {error}"))
}

// ------------------------------------------------------------------
// HostState — shared host introspection
// ------------------------------------------------------------------

pub const HOST_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize)]
pub struct HostState {
    pub schema_version: u32,
    pub binary: BinaryInfo,
    pub runtime_dir: RuntimeDirInfo,
    pub config_dir: ConfigDirInfo,
    pub daemon: DaemonInfo,
    pub service_unit: ServiceUnitInfo,
    pub cockpit: CockpitInfo,
    pub network: NetworkInfo,
}

#[derive(Clone, Debug, Serialize)]
pub struct BinaryInfo {
    pub path: Option<PathBuf>,
    pub version: &'static str,
    pub on_path: bool,
    pub shadowed_by: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeDirInfo {
    pub path: PathBuf,
    pub present: bool,
    pub config_file: FileEntry,
    pub database: FileEntry,
    pub logs_dir: DirEntry,
    pub run_dir: DirEntry,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConfigDirInfo {
    pub path: PathBuf,
    pub present: bool,
    pub entry_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DaemonInfo {
    pub state: ServiceState,
    pub bind: Option<String>,
    pub base_url: String,
    pub pid: Option<u32>,
    pub backend: ServiceBackend,
    pub healthy: bool,
    pub daemon_version: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServiceUnitInfo {
    pub backend: ServiceBackend,
    pub path: Option<PathBuf>,
    pub installed: bool,
    pub enabled: Option<bool>,
}

/// Cockpit-owned caches we'd remove on uninstall. v0.1.x has no on-disk
/// cockpit cache outside the runtime dir we already own; keep this as a
/// placeholder so the JSON shape is stable.
#[derive(Clone, Debug, Serialize)]
pub struct CockpitInfo {
    pub caches: Option<Vec<PathBuf>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NetworkInfo {
    pub bind: Option<String>,
    pub port: Option<u16>,
    pub port_in_use: PortInUse,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PortInUse {
    Free,
    InUse {
        pid: Option<u32>,
        command: Option<String>,
    },
    Unknown {
        reason: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub present: bool,
    pub size_bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DirEntry {
    pub path: PathBuf,
    pub present: bool,
    pub size_bytes: Option<u64>,
}

impl HostState {
    /// Collect host state. Sync, blocking; performs filesystem lookups,
    /// `which` walks, and a best-effort port probe. Acceptable for CLI startup.
    pub fn collect(paths: &RuntimePaths) -> Self {
        let binary = collect_binary_info();
        let runtime_dir = collect_runtime_dir(paths);
        let config_dir = collect_config_dir();
        let bind = read_configured_bind(paths);
        let port = bind.as_deref().and_then(parse_port);
        let (daemon, service_unit) = collect_daemon_and_unit(paths, bind.clone());
        let cockpit = CockpitInfo { caches: None };
        let network = NetworkInfo {
            bind: bind.clone(),
            port,
            port_in_use: probe_port(port, daemon.pid),
        };

        Self {
            schema_version: HOST_STATE_SCHEMA_VERSION,
            binary,
            runtime_dir,
            config_dir,
            daemon,
            service_unit,
            cockpit,
            network,
        }
    }
}

fn collect_binary_info() -> BinaryInfo {
    let current = std::env::current_exe().ok();
    let mut path_entries: Vec<PathBuf> = Vec::new();
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("asylum");
            if !candidate.is_file() {
                continue;
            }
            let canonical = fs::canonicalize(&candidate).unwrap_or(candidate.clone());
            if path_entries
                .iter()
                .any(|existing| fs::canonicalize(existing).unwrap_or(existing.clone()) == canonical)
            {
                continue;
            }
            path_entries.push(candidate);
        }
    }
    let on_path = !path_entries.is_empty();
    let shadowed_by = match (&current, path_entries.first()) {
        (Some(current), Some(first)) if same_file(current, first) => {
            path_entries.iter().skip(1).cloned().collect()
        }
        _ => path_entries.clone(),
    };
    BinaryInfo {
        path: current,
        version: env!("CARGO_PKG_VERSION"),
        on_path,
        shadowed_by,
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn collect_runtime_dir(paths: &RuntimePaths) -> RuntimeDirInfo {
    let logs_dir = paths.logs_dir();
    let run_dir = paths.run_dir();
    RuntimeDirInfo {
        present: paths.home.exists(),
        path: paths.home.clone(),
        config_file: file_entry(&paths.config),
        database: file_entry(&paths.database),
        logs_dir: dir_entry(&logs_dir),
        run_dir: dir_entry(&run_dir),
    }
}

fn collect_config_dir() -> ConfigDirInfo {
    let path = config_dir_path();
    let present = path.exists();
    let entry_count = if present {
        fs::read_dir(&path).map(|iter| iter.count()).unwrap_or(0)
    } else {
        0
    };
    ConfigDirInfo {
        path,
        present,
        entry_count,
    }
}

pub fn config_dir_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("asylum")
}

fn collect_daemon_and_unit(
    paths: &RuntimePaths,
    bind: Option<String>,
) -> (DaemonInfo, ServiceUnitInfo) {
    let manager = ServiceManager::new(paths.clone());
    let backend = manager
        .as_ref()
        .map(|manager| manager.backend())
        .unwrap_or_else(|_| select_backend());
    let state = manager
        .as_ref()
        .map(|manager| manager.status())
        .unwrap_or(ServiceState::Stopped);
    let pid = manager
        .as_ref()
        .ok()
        .and_then(|manager| manager.read_running_pid());
    let healthy = matches!(state, ServiceState::Running);
    let base_url = bind
        .as_deref()
        .and_then(|raw| raw.parse::<std::net::SocketAddr>().ok())
        .map(|addr| {
            let ip = if addr.ip().is_unspecified() {
                if addr.is_ipv6() {
                    std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
                } else {
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                }
            } else {
                addr.ip()
            };
            format!("http://{}", std::net::SocketAddr::new(ip, addr.port()))
        })
        .unwrap_or_else(|| "http://127.0.0.1:7717".to_string());

    let unit_path = manager.as_ref().ok().map(|manager| match backend {
        ServiceBackend::Launchd => manager.launchd_plist_location(),
        ServiceBackend::SystemdUser => manager.systemd_unit_location(),
        ServiceBackend::PidFallback => manager.paths_pid_metadata(),
    });
    let installed = unit_path
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false);
    let enabled = service_unit_enabled(backend);

    let service_unit = ServiceUnitInfo {
        backend,
        path: unit_path.filter(|_| backend != ServiceBackend::PidFallback),
        installed: installed && backend != ServiceBackend::PidFallback,
        enabled,
    };

    let daemon = DaemonInfo {
        state,
        bind,
        base_url,
        pid,
        backend,
        healthy,
        daemon_version: None,
    };
    (daemon, service_unit)
}

impl ServiceManager {
    fn paths_pid_metadata(&self) -> PathBuf {
        self.pid_metadata_path()
    }
}

fn service_unit_enabled(backend: ServiceBackend) -> Option<bool> {
    match backend {
        ServiceBackend::SystemdUser => {
            let output = ProcessCommand::new("systemctl")
                .arg("--user")
                .arg("is-enabled")
                .arg("asylum.service")
                .output()
                .ok()?;
            Some(output.status.success())
        }
        ServiceBackend::Launchd => {
            let output = ProcessCommand::new("launchctl")
                .arg("list")
                .arg(LABEL)
                .output()
                .ok()?;
            Some(output.status.success())
        }
        ServiceBackend::PidFallback => None,
    }
}

fn read_configured_bind(paths: &RuntimePaths) -> Option<String> {
    let content = fs::read_to_string(&paths.config).ok()?;
    let parsed: toml::Value = toml::from_str(&content).ok()?;
    parsed
        .get("listen")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn parse_port(bind: &str) -> Option<u16> {
    bind.parse::<std::net::SocketAddr>()
        .ok()
        .map(|addr| addr.port())
}

fn probe_port(port: Option<u16>, expected_pid: Option<u32>) -> PortInUse {
    let Some(port) = port else {
        return PortInUse::Unknown {
            reason: "no configured bind".to_string(),
        };
    };
    if !command_exists("lsof") {
        return PortInUse::Unknown {
            reason: "lsof not on PATH".to_string(),
        };
    }
    let output = match ProcessCommand::new("lsof")
        .arg("-nP")
        .arg(format!("-iTCP:{port}"))
        .arg("-sTCP:LISTEN")
        .arg("-Fpcn")
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return PortInUse::Unknown {
                reason: format!("lsof failed: {error}"),
            }
        }
    };
    if !output.status.success() && output.stdout.is_empty() {
        return PortInUse::Free;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut pid: Option<u32> = None;
    let mut command: Option<String> = None;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            pid = rest.trim().parse::<u32>().ok();
        } else if let Some(rest) = line.strip_prefix('c') {
            command = Some(rest.trim().to_string());
        }
    }
    if pid.is_none() && command.is_none() {
        return PortInUse::Free;
    }
    let _ = expected_pid;
    PortInUse::InUse { pid, command }
}

fn file_entry(path: &Path) -> FileEntry {
    let metadata = fs::metadata(path).ok();
    FileEntry {
        present: metadata.is_some(),
        size_bytes: metadata.as_ref().map(|m| m.len()),
        path: path.to_path_buf(),
    }
}

fn dir_entry(path: &Path) -> DirEntry {
    let present = path.is_dir();
    let size_bytes = if present { dir_size(path).ok() } else { None };
    DirEntry {
        path: path.to_path_buf(),
        present,
        size_bytes,
    }
}

fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                if let Ok(metadata) = entry.metadata() {
                    total = total.saturating_add(metadata.len());
                }
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn render_config() -> ServiceRenderConfig {
        ServiceRenderConfig {
            binary: PathBuf::from("/usr/local/bin/asylum"),
            config: PathBuf::from("/tmp/asylum/config.toml"),
            log: PathBuf::from("/tmp/asylum/logs/asylum.log"),
        }
    }

    #[test]
    fn launchd_renderer_uses_product_paths() {
        let plist = render_launchd_plist(&render_config());
        assert!(plist.contains("<string>/usr/local/bin/asylum</string>"));
        assert!(plist.contains("<string>daemon</string>"));
        assert!(plist.contains("<string>run</string>"));
        assert!(plist.contains("<string>--config</string>"));
        assert!(plist.contains("<string>/tmp/asylum/config.toml</string>"));
        assert!(plist.contains("<string>/tmp/asylum/logs/asylum.log</string>"));
    }

    #[test]
    fn systemd_renderer_uses_product_paths() {
        let unit = render_systemd_unit(&render_config());
        assert!(unit.contains(
            "ExecStart=\"/usr/local/bin/asylum\" daemon run --config \"/tmp/asylum/config.toml\""
        ));
        assert!(unit.contains("StandardOutput=append:/tmp/asylum/logs/asylum.log"));
    }

    #[test]
    fn systemd_renderer_escapes_spaces_and_specifiers() {
        let config = ServiceRenderConfig {
            binary: PathBuf::from("/opt/Asylum %bin/asylum"),
            config: PathBuf::from("/tmp/asylum config/config%.toml"),
            log: PathBuf::from("/tmp/asylum logs/asylum%.log"),
        };
        let unit = render_systemd_unit(&config);
        assert!(unit.contains("\"/opt/Asylum %%bin/asylum\""));
        assert!(unit.contains("\"/tmp/asylum config/config%%.toml\""));
        assert!(unit.contains("StandardOutput=append:/tmp/asylum\\x20logs/asylum%%.log"));
        assert!(unit.contains("StandardError=append:/tmp/asylum\\x20logs/asylum%%.log"));
    }

    #[test]
    fn systemd_execstart_args_escape_control_characters() {
        let rendered = systemd_quote_arg("/tmp/asylum\nbin/\u{0007}asylum");
        assert_eq!(rendered, "\"/tmp/asylum\\nbin/\\x07asylum\"");
        assert!(!rendered.contains('\n'));
        assert!(!rendered.contains('\u{0007}'));
    }

    #[test]
    fn refresh_installed_systemd_unit_file_rewrites_stale_serve_command() -> anyhow::Result<()> {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let tempdir = tempfile::tempdir()?;
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", tempdir.path());

        let result = (|| -> anyhow::Result<(Option<PathBuf>, PathBuf, String)> {
            let paths = RuntimePaths::from_values(
                Some(tempdir.path().join(".asylum")),
                None,
                None,
                Some(tempdir.path().join(".config").join("asylum")),
            );
            paths.ensure_dirs()?;
            let manager = ServiceManager::with_backend(
                paths,
                PathBuf::from("/home/test/.local/bin/asylum"),
                ServiceBackend::SystemdUser,
            );
            let unit_path = manager.systemd_unit_location();
            fs::create_dir_all(unit_path.parent().expect("unit parent"))?;
            fs::write(
                &unit_path,
                "ExecStart=\"/home/test/.local/bin/asylum\" serve --config \"/tmp/old.toml\"\n",
            )?;

            let refreshed = manager.refresh_installed_unit_file("127.0.0.1:7717")?;
            let unit = fs::read_to_string(&unit_path)?;
            Ok((refreshed, unit_path, unit))
        })();

        if let Some(previous_home) = previous_home {
            std::env::set_var("HOME", previous_home);
        } else {
            std::env::remove_var("HOME");
        }

        let (refreshed, unit_path, unit) = result?;
        assert_eq!(refreshed, Some(unit_path.clone()));
        assert!(unit.contains("ExecStart=\"/home/test/.local/bin/asylum\" daemon run --config"));
        assert!(!unit.contains(" serve "));
        Ok(())
    }

    #[test]
    fn pid_metadata_requires_matching_binary_pid_and_command() {
        let binary = PathBuf::from("/usr/local/bin/asylum");
        let content = "pid=42\nbinary=/usr/local/bin/asylum\ncommand=daemon run\n";
        assert!(pid_metadata_matches(&binary, 42, content));
        assert!(!pid_metadata_matches(&binary, 7, content));
        assert!(!pid_metadata_matches(
            &binary,
            42,
            "pid=42\nbinary=/bin/sleep\ncommand=daemon run\n"
        ));
        assert!(!pid_metadata_matches(
            &binary,
            42,
            "pid=42\nbinary=/usr/local/bin/asylum\ncommand=status\n"
        ));
    }

    #[test]
    fn pid_identity_prefers_metadata_when_argv_is_missing() {
        let binary = PathBuf::from("/usr/local/bin/asylum");
        let metadata = "pid=42\nbinary=/usr/local/bin/asylum\ncommand=daemon run\n";
        assert_eq!(
            classify_pid_identity(&binary, 42, Some(metadata), None),
            PidIdentity::Matches
        );
        assert_eq!(
            classify_pid_identity(
                &binary,
                42,
                Some(metadata),
                Some(&argv(&["/bin/sleep", "100"]))
            ),
            PidIdentity::Mismatch
        );
        assert_eq!(
            classify_pid_identity(
                &binary,
                42,
                Some(metadata),
                Some(&argv(&["/usr/local/bin/asylum", "daemon", "run"]))
            ),
            PidIdentity::Matches
        );
        assert_eq!(
            classify_pid_identity(
                &binary,
                42,
                None,
                Some(&argv(&["/usr/local/bin/asylum", "daemon", "run"]))
            ),
            PidIdentity::Unknown
        );
        assert_eq!(
            classify_pid_identity(
                &binary,
                42,
                Some("pid=42\nbinary=/bin/asylum\ncommand=daemon run\n"),
                None
            ),
            PidIdentity::Mismatch
        );
    }

    #[test]
    fn command_argv_matching_requires_executable_identity() {
        let binary = PathBuf::from("/usr/local/bin/asylum");
        assert!(command_argv_matches_asylum(
            &binary,
            &argv(&["/usr/local/bin/asylum", "daemon", "run"])
        ));
        assert!(command_argv_matches_asylum(
            &binary,
            &argv(&["asylum", "daemon", "run"])
        ));
        assert!(!command_argv_matches_asylum(
            &binary,
            &argv(&["/usr/local/bin/asylum-helper", "daemon", "run"])
        ));
        assert!(!command_argv_matches_asylum(
            &binary,
            &argv(&["sh", "-c", "/usr/local/bin/asylum daemon run"])
        ));
        assert!(!command_argv_matches_asylum(
            &binary,
            &argv(&[
                "/usr/local/bin/asylum-helper",
                "--old",
                "/usr/local/bin/asylum",
                "daemon",
                "run"
            ])
        ));
        assert!(!command_argv_matches_asylum(
            &binary,
            &argv(&["/usr/local/bin/asylum daemon run"])
        ));
        assert!(!command_argv_matches_asylum(
            &binary,
            &argv(&["/usr/local/bin/asylum", "daemon run"])
        ));
        assert!(command_argv_matches_asylum(
            &PathBuf::from("/Applications/Asylum Bin/asylum"),
            &argv(&["/Applications/Asylum Bin/asylum", "daemon", "run"])
        ));
    }

    #[test]
    fn healthy_service_state_wins_for_status() {
        assert_eq!(
            service_state_from_health(true, ServiceState::Stopped),
            ServiceState::Running
        );
        assert_eq!(
            service_state_from_health(false, ServiceState::Stopped),
            ServiceState::Stopped
        );
    }

    #[test]
    fn host_state_collects_for_empty_runtime() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let home = tempdir.path().join("nope");
        let paths =
            RuntimePaths::from_values(Some(home), None, None, Some(tempdir.path().to_path_buf()));
        let state = HostState::collect(&paths);
        assert_eq!(state.schema_version, HOST_STATE_SCHEMA_VERSION);
        assert!(!state.runtime_dir.present);
        assert!(!state.runtime_dir.config_file.present);
        assert!(!state.runtime_dir.database.present);
    }

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }
}
