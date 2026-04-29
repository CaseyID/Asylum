use std::collections::HashMap;
use std::sync::Arc;

use crate::harness::claude::ClaudeHarness;
use crate::harness::codex::CodexHarness;
use asylum_core::config::HarnessConfig;
use asylum_core::node::{CapabilitySnapshot, HarnessKind};

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
    fn launch_context(&self, request: &asylum_core::api::CreateNodeRequest) -> String;
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
    let key = match kind {
        HarnessKind::Codex => "codex",
        HarnessKind::ClaudeCode => "claude_code",
    };
    config.startup_args.get(key).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use asylum_core::node::HarnessKind;

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
}
