use std::fs;
use std::io::Write;

use asylum_types::node::CapabilitySnapshot;
use asylum_types::node::HarnessKind;

use crate::harness::launch_context::LaunchContext;

pub struct CodexHarness {
    command: String,
    launch_args: Vec<String>,
}

impl CodexHarness {
    pub fn new(command: String, launch_args: Vec<String>) -> Self {
        Self {
            command,
            launch_args,
        }
    }
}

impl super::HarnessAdapter for CodexHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Codex
    }

    fn command(&self) -> &str {
        &self.command
    }

    fn launch_args(&self) -> &[String] {
        &self.launch_args
    }

    fn capabilities(&self) -> CapabilitySnapshot {
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

    fn launch_context(
        &self,
        node_id: uuid::Uuid,
        request: &asylum_types::api::CreateNodeRequest,
    ) -> String {
        let context = LaunchContext {
            node_id,
            workspace: request.workspace.clone().map(std::path::PathBuf::from),
            role_hint: request.role_hint.clone(),
            graph_summary: "No relationships by default".to_string(),
            capabilities: vec![
                "send_input".to_string(),
                "interrupt".to_string(),
                "stop".to_string(),
            ],
        };
        context.instruction_prompt()
    }

    fn asylum_control_args(
        &self,
        asylum_binary: &str,
        socket_path: Option<&str>,
        node_id: uuid::Uuid,
        // Codex session ids are not pre-assignable (`codex` has no --session-id);
        // W1 records the thread-id from the first notify post instead.
        _session_id: Option<uuid::Uuid>,
    ) -> Vec<String> {
        let mut env_entries = vec![format!(
            "ASYLUM_NODE_ID={}",
            toml_string(&node_id.to_string())
        )];
        if let Some(socket_path) = socket_path {
            env_entries.push(format!("ASYLUM_SOCKET_PATH={}", toml_string(socket_path)));
        }

        vec![
            "-c".to_string(),
            format!("mcp_servers.asylum.command={}", toml_string(asylum_binary)),
            "-c".to_string(),
            "mcp_servers.asylum.args=[\"mcp\"]".to_string(),
            "-c".to_string(),
            format!("mcp_servers.asylum.env={{{}}}", env_entries.join(",")),
            "-c".to_string(),
            "mcp_servers.asylum.required=true".to_string(),
            "-c".to_string(),
            "mcp_servers.asylum.startup_timeout_sec=10".to_string(),
            "-c".to_string(),
            "mcp_servers.asylum.tool_timeout_sec=60".to_string(),
            // Route codex's per-turn `agent-turn-complete` notification through the
            // asylum bridge. Codex appends one argv element of JSON to this command;
            // the bridge reads it from argv (never stdin) and POSTs the mapped event.
            // Value is a TOML array of strings, matching how `-c mcp_servers.*.args`
            // is formatted above.
            "-c".to_string(),
            format!(
                "notify=[{},{},{}]",
                toml_string(asylum_binary),
                toml_string("harness-event"),
                toml_string("codex-notify")
            ),
        ]
    }

    fn pre_trust_workspace(&self, workspace: &str) -> anyhow::Result<()> {
        // Upsert ~/.codex/config.toml so codex skips the "Do you trust this directory?"
        // dialog. The key is [projects."<absolute-workspace-path>"] trust_level = "trusted".
        //
        // Codex resolves trust at the git repository root when the workspace is inside a
        // git repo (it walks up looking for .git and trusts that ancestor, not the
        // workspace dir itself). We mirror that logic here so the pre-trust matches what
        // codex will look up at startup.
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine HOME"))?;
        let config_path = home.join(".codex").join("config.toml");

        // Collect the set of paths to trust: the workspace itself, plus the git root if
        // the workspace is inside a git repo.
        let mut paths_to_trust: Vec<String> = vec![workspace.to_string()];
        let git_root = super::find_git_root(std::path::Path::new(workspace));
        if let Some(root) = git_root {
            let root_str = root.to_string_lossy().to_string();
            if root_str != workspace {
                paths_to_trust.push(root_str);
            }
        }

        // Read existing content (empty string if the file doesn't exist yet).
        let raw = if config_path.exists() {
            fs::read_to_string(&config_path)?
        } else {
            String::new()
        };

        let mut doc: toml::Value = if raw.is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            raw.parse::<toml::Value>()?
        };

        let table = doc
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("codex config is not a TOML table"))?;
        let projects = table
            .entry("projects")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        let projects_table = projects
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("codex config [projects] is not a table"))?;

        let mut any_changed = false;
        for path in &paths_to_trust {
            let entry = projects_table
                .entry(path.clone())
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            let entry_table = entry
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("codex config project entry is not a table"))?;
            if entry_table.get("trust_level").and_then(|v| v.as_str()) != Some("trusted") {
                entry_table.insert(
                    "trust_level".to_string(),
                    toml::Value::String("trusted".to_string()),
                );
                any_changed = true;
            }
        }

        if !any_changed {
            return Ok(());
        }

        // Write atomically: temp file in same dir then rename.
        let dir = config_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("config path has no parent"))?;
        fs::create_dir_all(dir)?;
        let tmp_path = config_path.with_extension("toml.tmp");
        {
            let mut f = fs::File::create(&tmp_path)?;
            f.write_all(toml::to_string_pretty(&doc)?.as_bytes())?;
            f.flush()?;
        }
        fs::rename(&tmp_path, &config_path)?;

        tracing::debug!(workspace = workspace, paths = ?paths_to_trust, "pre-trusted codex workspace");
        Ok(())
    }
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
