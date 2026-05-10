use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use asylum_types::api::SubstrateHealth;
use asylum_types::node::{CapabilitySnapshot, HarnessKind};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct LoonSubstrate {
    endpoint: String,
    client: Client,
    cli_path: Option<PathBuf>,
    api_key_file: Option<PathBuf>,
    cert_fingerprint_file: Option<PathBuf>,
    enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoonHealth {
    pub status: String,
    pub running_instances: Option<usize>,
    pub harness_profiles: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct LoonContext {
    pub node_id: Uuid,
    pub harness: HarnessKind,
    pub command: String,
    pub prompt: String,
}

pub fn capability_flags_from_health(
    health: &LoonHealth,
    harness: &HarnessKind,
) -> CapabilitySnapshot {
    let profile_supported = match &health.harness_profiles {
        Some(profiles) => match harness {
            HarnessKind::Codex => profiles.iter().any(|value| value == "codex"),
            HarnessKind::ClaudeCode => profiles.iter().any(|value| value == "claude_code"),
        },
        None => false,
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
        resume: false,
        structured_events: false,
        transcript_export: false,
    }
}

impl LoonSubstrate {
    pub fn new(
        endpoint: &str,
        cli_path: Option<PathBuf>,
        api_key_file: Option<PathBuf>,
        cert_fingerprint_file: Option<PathBuf>,
        enabled: bool,
    ) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client: Client::new(),
            cli_path,
            api_key_file,
            cert_fingerprint_file,
            enabled,
        }
    }

    pub async fn health(&self) -> Result<LoonHealth> {
        if !self.enabled {
            return Ok(LoonHealth {
                status: "disabled".to_string(),
                running_instances: None,
                harness_profiles: None,
            });
        }
        if let Ok(output) = self.run_cli(vec!["version".to_string()]).await {
            if let Some(health) = parse_loon_cli_health(&output) {
                return Ok(health);
            }
            return Ok(LoonHealth {
                status: "limited".to_string(),
                running_instances: None,
                harness_profiles: None,
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
            status: "limited".to_string(),
            running_instances: None,
            harness_profiles: None,
        })
    }

    pub async fn check_support(&self, harness: &HarnessKind) -> Result<()> {
        let health = self.health().await?;
        if !capability_flags_from_health(&health, harness).send_input {
            return Err(anyhow!("unsupported_on_substrate"));
        }
        Ok(())
    }

    pub async fn launch_node(&self, context: &LoonContext) -> Result<String> {
        if !self.enabled {
            return Err(anyhow!("loon disabled"));
        }
        self.check_support(&self.current_harness_from_command(&context.command)?)
            .await?;
        let output = self
            .run_cli(vec![
                "spawn".to_string(),
                "--prompt".to_string(),
                context.prompt.clone(),
            ])
            .await?;
        parse_spawned_instance_id(&output)
            .ok_or_else(|| anyhow!("loon spawn did not return an instance id"))
    }

    pub async fn send_input(&self, external_id: &str, text: &str) -> Result<()> {
        self.run_cli(vec![
            "tell".to_string(),
            external_id.to_string(),
            text.to_string(),
        ])
        .await?;
        Ok(())
    }

    pub async fn interrupt(&self, external_id: &str) -> Result<()> {
        self.run_cli(vec!["interrupt".to_string(), external_id.to_string()])
            .await?;
        Ok(())
    }

    pub async fn stop(&self, external_id: &str) -> Result<()> {
        self.run_cli(vec!["stop".to_string(), external_id.to_string()])
            .await?;
        Ok(())
    }

    pub async fn stop_node(&self, external_id: &str) -> Result<()> {
        self.stop(external_id).await
    }

    pub async fn archive(&self, external_id: &str) -> Result<()> {
        self.run_cli(vec!["terminate".to_string(), external_id.to_string()])
            .await?;
        Ok(())
    }

    pub fn attach_invocation(
        &self,
        external_id: &str,
    ) -> (PathBuf, Vec<String>, Vec<(String, String)>) {
        (
            self.cli_binary(),
            vec!["attach".to_string(), external_id.to_string()],
            self.cli_env(),
        )
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
                running_instances: None,
                harness_profiles: None,
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

    async fn run_cli(&self, args: Vec<String>) -> Result<String> {
        let mut command = Command::new(self.cli_binary());
        command.args(args);
        for (key, value) in self.cli_env() {
            command.env(key, value);
        }
        let output = command.output().await?;
        if !output.status.success() {
            return Err(anyhow!(
                "loon CLI failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn cli_binary(&self) -> PathBuf {
        self.cli_path
            .clone()
            .unwrap_or_else(|| Path::new("loon").to_path_buf())
    }

    fn cli_env(&self) -> Vec<(String, String)> {
        let mut env = vec![("LOON_ENDPOINT".to_string(), self.endpoint.clone())];
        if let Some(path) = &self.api_key_file {
            env.push(("LOON_API_KEY_FILE".to_string(), path.display().to_string()));
        }
        if let Some(path) = &self.cert_fingerprint_file {
            env.push((
                "LOON_CERT_FINGERPRINT_FILE".to_string(),
                path.display().to_string(),
            ));
        }
        env
    }
}

fn parse_loon_cli_health(output: &str) -> Option<LoonHealth> {
    let trimmed = output.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None;
    }
    serde_json::from_str::<LoonHealth>(trimmed).ok()
}

fn parse_spawned_instance_id(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.split_whitespace().next())
        .filter(|value| Uuid::parse_str(value).is_ok())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_loon_cli_spawn_and_version_output() {
        assert_eq!(
            parse_spawned_instance_id("00000000-0000-0000-0000-000000000000\trunning\n"),
            Some("00000000-0000-0000-0000-000000000000".to_string())
        );
    }

    #[test]
    fn loon_capabilities_require_known_harness_profiles() {
        let health = LoonHealth {
            status: "ok".to_string(),
            running_instances: None,
            harness_profiles: None,
        };

        let snapshot = capability_flags_from_health(&health, &HarnessKind::Codex);

        assert!(snapshot.browser_attach);
        assert!(!snapshot.send_input);
        assert!(!snapshot.structured_events);
    }

    #[tokio::test]
    async fn health_reports_version_checks_as_limited_measurement() {
        let workdir = tempfile::tempdir().expect("tempdir");
        let script_path = workdir.path().join("loon-version.sh");
        std::fs::write(&script_path, "#!/bin/sh\nprintf 'loon 0.1.0\\n'\nexit 0\n")
            .expect("write script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path)
                .expect("metadata")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).expect("chmod");
        }

        let loon = LoonSubstrate::new(
            "http://127.0.0.1:0",
            Some(Path::new(&script_path).to_path_buf()),
            None,
            None,
            true,
        );

        let health = loon.health().await.expect("health");
        assert_eq!(health.status, "limited");
        assert!(health.running_instances.is_none());
        assert!(health.harness_profiles.is_none());
    }
}
