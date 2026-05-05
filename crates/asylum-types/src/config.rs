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
    pub endpoint: String,
    pub api_key_file: Option<PathBuf>,
    pub cert_fingerprint_file: Option<PathBuf>,
    pub cli_path: Option<PathBuf>,
    pub enabled: bool,
}

impl Default for LoonConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:7777".to_string(),
            api_key_file: None,
            cert_fingerprint_file: None,
            cli_path: None,
            enabled: false,
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
