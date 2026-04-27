use anyhow::{anyhow, Result};
use asylum_core::api::SubstrateHealth;
use asylum_core::node::{CapabilitySnapshot, HarnessKind};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct LoonSubstrate {
    endpoint: String,
    client: Client,
    _cli_path: Option<std::path::PathBuf>,
    enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoonHealth {
    pub status: String,
    pub running_instances: usize,
    pub harness_profiles: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct LoonContext {
    pub node_id: Uuid,
    pub harness: HarnessKind,
    pub command: String,
}

pub fn capability_flags_from_health(
    health: &LoonHealth,
    harness: &HarnessKind,
) -> CapabilitySnapshot {
    let profile_supported = match harness {
        HarnessKind::Codex => health.harness_profiles.iter().any(|value| value == "codex"),
        HarnessKind::ClaudeCode => health
            .harness_profiles
            .iter()
            .any(|value| value == "claude_code"),
    };
    if !profile_supported {
        return CapabilitySnapshot {
            browser_attach: true,
            native_attach: true,
            send_input: false,
            interrupt: false,
            stop: false,
            resume: false,
            structured_events: false,
            transcript_export: false,
        };
    }

    CapabilitySnapshot {
        browser_attach: true,
        native_attach: true,
        send_input: true,
        interrupt: true,
        stop: true,
        resume: true,
        structured_events: true,
        transcript_export: false,
    }
}

impl LoonSubstrate {
    pub fn new(endpoint: &str, cli_path: Option<std::path::PathBuf>, enabled: bool) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client: Client::new(),
            _cli_path: cli_path,
            enabled,
        }
    }

    pub async fn health(&self) -> Result<LoonHealth> {
        if !self.enabled {
            return Ok(LoonHealth {
                status: "disabled".to_string(),
                running_instances: 0,
                harness_profiles: vec![],
            });
        }
        let response = self
            .client
            .get(format!("{}/version", self.endpoint))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!("loon unreachable"));
        }
        Ok(LoonHealth {
            status: "ok".to_string(),
            running_instances: 0,
            harness_profiles: vec!["claude_code".to_string()],
        })
    }

    pub async fn check_support(&self, harness: &HarnessKind) -> Result<()> {
        let health = self.health().await?;
        if !capability_flags_from_health(&health, harness).send_input {
            return Err(anyhow!("unsupported_on_substrate"));
        }
        Ok(())
    }

    pub async fn launch_node(&self, _context: &LoonContext) -> Result<String> {
        if !self.enabled {
            return Err(anyhow!("loon disabled"));
        }
        self.check_support(&self.current_harness_from_command(&_context.command)?)
            .await?;
        Ok(format!("loon-{}", _context.node_id))
    }

    pub async fn send_input(&self, _external_id: &str, _text: &str) -> Result<()> {
        Ok(())
    }

    pub async fn interrupt(&self, _external_id: &str) -> Result<()> {
        Ok(())
    }

    pub async fn stop(&self, _external_id: &str) -> Result<()> {
        Ok(())
    }

    pub async fn stop_node(&self, _external_id: &str) -> Result<()> {
        Ok(())
    }

    pub async fn archive(&self, _external_id: &str) -> Result<()> {
        Ok(())
    }

    pub async fn health_api(&self) -> SubstrateHealth {
        match self.health().await {
            Ok(health) => SubstrateHealth {
                status: health.status,
                running_instances: health.running_instances,
                harness_profiles: health.harness_profiles,
            },
            Err(_) => SubstrateHealth {
                status: "unavailable".to_string(),
                running_instances: 0,
                harness_profiles: vec![],
            },
        }
    }

    fn current_harness_from_command(&self, command: &str) -> Result<HarnessKind> {
        match command {
            "codex" => Ok(HarnessKind::Codex),
            "claude" | "claude_code" => Ok(HarnessKind::ClaudeCode),
            _ => Err(anyhow!("unknown harness command")),
        }
    }
}
