use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::harness::claude::ClaudeHarness;
use crate::harness::codex::CodexHarness;
use asylum_types::config::HarnessConfig;
use asylum_types::node::{CapabilitySnapshot, HarnessKind};

mod claude;
mod codex;
pub mod launch_context;

#[derive(Debug)]
pub enum HarnessError {
    UnknownHarness,
}

pub trait HarnessAdapter: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn command(&self) -> &str;
    fn launch_args(&self) -> &[String];
    fn capabilities(&self) -> CapabilitySnapshot;
    fn launch_context(&self, request: &asylum_types::api::CreateNodeRequest) -> String;
    /// Idempotently record the workspace path as trusted in the harness's own config
    /// so the first-run trust dialog is skipped when the process spawns.
    fn pre_trust_workspace(&self, workspace: &str) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct HarnessRegistry {
    adapters: HashMap<HarnessKind, Arc<dyn HarnessAdapter>>,
}

impl HarnessRegistry {
    pub fn from_config(config: &HarnessConfig) -> Self {
        let mut adapters: HashMap<HarnessKind, Arc<dyn HarnessAdapter>> = HashMap::new();
        adapters.insert(
            HarnessKind::Codex,
            Arc::new(CodexHarness::new(
                config.codex_command.clone(),
                launch_args_for(config, &HarnessKind::Codex),
            )),
        );
        adapters.insert(
            HarnessKind::ClaudeCode,
            Arc::new(ClaudeHarness::new(
                config.claude_command.clone(),
                launch_args_for(config, &HarnessKind::ClaudeCode),
            )),
        );
        Self { adapters }
    }

    pub fn get(&self, kind: &HarnessKind) -> Option<&Arc<dyn HarnessAdapter>> {
        self.adapters.get(kind)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn HarnessAdapter>> {
        self.adapters.values()
    }
}

impl Default for HarnessRegistry {
    fn default() -> Self {
        Self::from_config(&HarnessConfig::default())
    }
}

fn launch_args_for(config: &HarnessConfig, kind: &HarnessKind) -> Vec<String> {
    // Bake in trust-bypass flags by default. The user already accepted the workspace
    // path when they typed it into Asylum; re-prompting inside the harness is pure
    // friction. Both flags can be overridden at the config level via startup_args if
    // the operator wants a stricter policy.
    let mut args = match kind {
        HarnessKind::Codex => {
            // --dangerously-bypass-approvals-and-sandbox: skip all confirmation prompts
            // and sandbox restrictions. Codex's own help: "Intended solely for running
            // in environments that are externally sandboxed" — Asylum is that environment.
            vec!["--dangerously-bypass-approvals-and-sandbox".to_string()]
        }
        HarnessKind::ClaudeCode => {
            // --dangerously-skip-permissions: bypass all permission checks.
            // Claude's own help: "Recommended only for sandboxes with no internet access."
            vec!["--dangerously-skip-permissions".to_string()]
        }
    };

    let key = match kind {
        HarnessKind::Codex => "codex",
        HarnessKind::ClaudeCode => "claude_code",
    };
    args.extend(config.startup_args.get(key).cloned().unwrap_or_default());
    args
}

/// Walk up from `start` looking for a `.git` entry. Returns the directory that
/// contains `.git` (i.e. the repo root), or `None` if none is found before the
/// filesystem root.
pub(crate) fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        match dir.parent() {
            Some(p) if p != dir => dir = p.to_path_buf(),
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asylum_types::node::HarnessKind;

    #[test]
    fn default_harness_commands_are_real_clis() {
        let registry = HarnessRegistry::default();
        let codex = registry.get(&HarnessKind::Codex).unwrap();
        let claude = registry.get(&HarnessKind::ClaudeCode).unwrap();

        assert_eq!(codex.command(), "codex");
        assert_eq!(claude.command(), "claude");
        assert!(codex.capabilities().send_input);
        assert!(claude.capabilities().send_input);
    }

    #[test]
    fn default_launch_args_include_trust_bypass_flags() {
        let registry = HarnessRegistry::default();
        let codex = registry.get(&HarnessKind::Codex).unwrap();
        let claude = registry.get(&HarnessKind::ClaudeCode).unwrap();

        assert!(
            codex
                .launch_args()
                .contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()),
            "codex must skip confirmation prompts by default"
        );
        assert!(
            claude
                .launch_args()
                .contains(&"--dangerously-skip-permissions".to_string()),
            "claude must skip permission prompts by default"
        );
    }

    #[test]
    fn config_startup_args_are_appended_after_defaults() {
        let mut config = HarnessConfig::default();
        config
            .startup_args
            .insert("codex".to_string(), vec!["--model".to_string(), "o3".to_string()]);

        let registry = HarnessRegistry::from_config(&config);
        let codex = registry.get(&HarnessKind::Codex).unwrap();
        let args = codex.launch_args();

        // Default flag must come first
        assert_eq!(args[0], "--dangerously-bypass-approvals-and-sandbox");
        // Config-supplied args follow
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"o3".to_string()));
    }
}
