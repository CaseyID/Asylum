use std::fs;
use std::io::Write;

use asylum_types::node::CapabilitySnapshot;
use asylum_types::node::HarnessKind;
use serde_json::Value;

use crate::harness::launch_context::LaunchContext;

pub struct ClaudeHarness {
    command: String,
    launch_args: Vec<String>,
}

impl ClaudeHarness {
    pub fn new(command: String, launch_args: Vec<String>) -> Self {
        Self {
            command,
            launch_args,
        }
    }
}

impl super::HarnessAdapter for ClaudeHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::ClaudeCode
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
            graph_summary: "Graph edges are explicit only.".to_string(),
            capabilities: vec![
                "send_input".to_string(),
                "interrupt".to_string(),
                "stop".to_string(),
            ],
        };
        context.instruction_prompt()
    }

    fn pre_trust_workspace(&self, workspace: &str) -> anyhow::Result<()> {
        // Upsert ~/.claude.json so claude skips the workspace trust dialog.
        // The key is projects[<absolute-workspace-path>].hasTrustDialogAccepted = true.
        //
        // Claude resolves trust at the git repository root when the workspace lives
        // inside a git repo. We trust both the workspace path and the git root so the
        // lookup succeeds regardless of which path claude resolves internally.
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine HOME"))?;
        let config_path = home.join(".claude.json");

        // Collect the set of paths to trust.
        let mut paths_to_trust: Vec<String> = vec![workspace.to_string()];
        let git_root = super::find_git_root(std::path::Path::new(workspace));
        if let Some(root) = git_root {
            let root_str = root.to_string_lossy().to_string();
            if root_str != workspace {
                paths_to_trust.push(root_str);
            }
        }

        // Read existing content (empty object if file doesn't exist).
        let raw = if config_path.exists() {
            fs::read_to_string(&config_path)?
        } else {
            String::new()
        };

        let mut root: Value = if raw.is_empty() {
            Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&raw)?
        };

        let root_obj = root
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("~/.claude.json root is not a JSON object"))?;

        let projects = root_obj
            .entry("projects")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let projects_obj = projects
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("~/.claude.json projects is not an object"))?;

        let mut any_changed = false;
        for path in &paths_to_trust {
            let project = projects_obj
                .entry(path.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            let project_obj = project
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("project entry is not an object"))?;
            if project_obj
                .get("hasTrustDialogAccepted")
                .and_then(|v| v.as_bool())
                != Some(true)
            {
                project_obj.insert("hasTrustDialogAccepted".to_string(), Value::Bool(true));
                any_changed = true;
            }
        }

        if !any_changed {
            return Ok(());
        }

        // Atomic write: temp file then rename.
        let tmp_path = config_path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp_path)?;
            f.write_all(serde_json::to_string_pretty(&root)?.as_bytes())?;
            f.flush()?;
        }
        fs::rename(&tmp_path, &config_path)?;

        tracing::debug!(workspace = workspace, paths = ?paths_to_trust, "pre-trusted claude workspace");
        Ok(())
    }
}
