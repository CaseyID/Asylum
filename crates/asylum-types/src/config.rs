use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsylumConfig {
    pub listen: Option<String>,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub harness: HarnessConfig,
    #[serde(default)]
    pub loon: LoonConfig,
    #[serde(default)]
    pub ntfy: NtfyConfig,
    #[serde(default)]
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    pub autonomy: AutonomyConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsylumFileConfig {
    #[serde(flatten)]
    pub core: AsylumConfig,
    pub database: String,
}

impl Default for AsylumConfig {
    fn default() -> Self {
        Self {
            listen: Some("127.0.0.1:7717".to_string()),
            base_url: "http://127.0.0.1:7717".to_string(),
            auth: AuthConfig::default(),
            harness: HarnessConfig::default(),
            loon: LoonConfig::default(),
            ntfy: NtfyConfig::default(),
            workspace: WorkspaceConfig::default(),
            autonomy: AutonomyConfig::default(),
        }
    }
}

/// Tunables for the daemon-side autonomy signals (Phase B).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutonomyConfig {
    /// Context-window usage percentages (0-100) that fire `node.ctx_pressure`
    /// when crossed. Each threshold fires at most once per harness session.
    pub ctx_pressure_thresholds: Vec<f64>,
    /// Seconds of no PTY output on a Running local node before the quiescence
    /// timer fires `node.idle` (only for harnesses without a native idle signal).
    pub idle_quiescence_seconds: u64,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            ctx_pressure_thresholds: vec![75.0, 90.0],
            idle_quiescence_seconds: 120,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuthConfig {
    pub owner_tokens_enabled: bool,
    pub owner_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarnessConfig {
    #[serde(default)]
    pub codex_command: String,
    #[serde(default)]
    pub claude_command: String,
    #[serde(default)]
    pub default_workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub startup_args: BTreeMap<String, Vec<String>>,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            codex_command: "codex".to_string(),
            claude_command: "claude".to_string(),
            default_workspace_root: None,
            startup_args: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoonConfig {
    #[serde(default = "default_loon_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub api_key_file: Option<PathBuf>,
    #[serde(default)]
    pub cert_fingerprint_file: Option<PathBuf>,
    #[serde(default)]
    pub cli_path: Option<PathBuf>,
    #[serde(default)]
    pub enabled: bool,
    /// Path to the loon client config.toml (url/key/fingerprint per profile).
    /// Defaults to \$XDG_CONFIG_HOME/loon/config.toml (or ~/.config/loon/config.toml).
    #[serde(default)]
    pub config_path: Option<PathBuf>,
    /// loon profile name to use from the client config. Defaults to the config's
    /// default_profile.
    #[serde(default)]
    pub profile: Option<String>,
    /// Guest OCI-tar image path (local path the loon host can read) used for
    /// . Defaults to the claude-dev reference image.
    #[serde(default = "default_loon_image")]
    pub image: String,
    /// Base URL the in-guest harness uses to reach the Asylum daemon over HTTP.
    /// Guests reach the host via the per-VM gateway, stably named
    /// host.loon.internal. Defaults to http://host.loon.internal:<asylum-port>
    /// derived from the daemon bind when unset.
    #[serde(default)]
    pub guest_base_url: Option<String>,
    /// In-guest workspace directory created at provision when a node does not
    /// specify a workspace. Loon workspaces live INSIDE the guest (no host bind
    /// mounts). Defaults to /work.
    #[serde(default = "default_loon_workspace")]
    pub workspace_dir: String,
    /// microVM memory in MiB for `loon vm create`. claude-code (Node.js) plus the
    /// in-guest MCP server need well beyond loon's 256 MiB default; too little
    /// OOMs the guest. Defaults to 2048.
    #[serde(default = "default_loon_vm_memory_mib")]
    pub vm_memory_mib: u32,
    /// microVM vCPU count for `loon vm create`. Defaults to 2.
    #[serde(default = "default_loon_vm_cpus")]
    pub vm_cpus: u32,
    /// Host path to the static musl \`asylum\` binary staged into the guest at
    /// /usr/local/bin/asylum for the in-guest MCP server + harness-event bridge.
    /// Required for MCP-in-guest; build via scripts/build-guest-asylum.sh.
    #[serde(default)]
    pub guest_asylum_binary: Option<PathBuf>,
}

fn default_loon_endpoint() -> String {
    "http://127.0.0.1:7777".to_string()
}

fn default_loon_image() -> String {
    "/var/lib/loon/agent-images/claude-dev.oci.tar".to_string()
}

fn default_loon_workspace() -> String {
    "/work".to_string()
}

fn default_loon_vm_memory_mib() -> u32 {
    2048
}

fn default_loon_vm_cpus() -> u32 {
    2
}

impl Default for LoonConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:7777".to_string(),
            api_key_file: None,
            cert_fingerprint_file: None,
            cli_path: None,
            enabled: false,
            config_path: None,
            profile: None,
            image: default_loon_image(),
            guest_base_url: None,
            workspace_dir: default_loon_workspace(),
            vm_memory_mib: default_loon_vm_memory_mib(),
            vm_cpus: default_loon_vm_cpus(),
            guest_asylum_binary: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NtfyConfig {
    pub server: Option<String>,
    pub topic: Option<String>,
    pub token: Option<String>,
    pub poll_interval_seconds: u64,
}

impl Default for NtfyConfig {
    fn default() -> Self {
        Self {
            server: None,
            topic: None,
            token: None,
            poll_interval_seconds: 30,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub recent_limit: usize,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self { recent_limit: 20 }
    }
}
