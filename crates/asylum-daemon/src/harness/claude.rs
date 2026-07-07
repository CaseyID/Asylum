use std::fs;
use std::io::Write;

use asylum_types::node::CapabilitySnapshot;
use asylum_types::node::HarnessKind;
use serde_json::Value;

use crate::harness::launch_context::LaunchContext;
use crate::harness::DaemonResolution;

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

    fn native_idle_signal(&self) -> bool {
        // claude emits idle via the Notification `idle_prompt` hook.
        true
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

    fn preassign_session_id(&self) -> Option<uuid::Uuid> {
        // claude accepts a caller-chosen session id via `--session-id`. Pre-assigning
        // it makes the resume key known at create time (recorded on the node row).
        Some(uuid::Uuid::new_v4())
    }

    fn asylum_control_args(
        &self,
        asylum_binary: &str,
        resolution: &DaemonResolution,
        node_id: uuid::Uuid,
        session_id: Option<uuid::Uuid>,
    ) -> Vec<String> {
        let mut env = serde_json::Map::new();
        env.insert(
            "ASYLUM_NODE_ID".to_string(),
            Value::String(node_id.to_string()),
        );
        match resolution {
            DaemonResolution::Socket(Some(socket_path)) => {
                env.insert(
                    "ASYLUM_SOCKET_PATH".to_string(),
                    Value::String(socket_path.to_string()),
                );
            }
            DaemonResolution::Socket(None) => {}
            DaemonResolution::Http { base_url, token } => {
                env.insert(
                    "ASYLUM_BASE_URL".to_string(),
                    Value::String(base_url.to_string()),
                );
                env.insert(
                    "ASYLUM_TOKEN".to_string(),
                    Value::String(token.to_string()),
                );
            }
        }
        let mcp_config = serde_json::json!({
            "mcpServers": {
                "asylum": {
                    "type": "stdio",
                    "command": asylum_binary,
                    "args": ["mcp"],
                    "env": env,
                }
            }
        })
        .to_string();
        let mut args = vec![
            "--mcp-config".to_string(),
            mcp_config,
            "--strict-mcp-config".to_string(),
            "--allowedTools".to_string(),
            "mcp__asylum__*".to_string(),
        ];
        // Pre-assigned resume key (Phase C `claude --resume <id>`). Recorded on the
        // node row at create time; the SessionStart hook posts the same id back, so
        // W1's ingestion is a no-op confirm.
        if let Some(session_id) = session_id {
            args.push("--session-id".to_string());
            args.push(session_id.to_string());
        }
        // Inject reporting hooks + statusline inline for this run only. The commands
        // invoke the same asylum binary as the MCP injection; ASYLUM_NODE_ID and
        // ASYLUM_SOCKET_PATH are already set on the launched process environment, so
        // the hook child processes inherit them and the bridge can resolve the node.
        args.push("--settings".to_string());
        args.push(claude_settings_json(asylum_binary));
        args
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

/// POSIX single-quote a string so it survives being embedded in a shell command
/// line (claude runs hook / statusline `command` values via the shell).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Build the inline `--settings` JSON that wires claude's reporting hooks and
/// statusline through the asylum bridge. Every hook invokes
/// `<asylum-binary> harness-event claude-hook`; the statusline invokes
/// `<asylum-binary> harness-event claude-statusline`. The daemon maps each event
/// by payload content (hook_event_name / Notification `type` / SessionStart
/// `source`), so no matchers are set — every occurrence is forwarded.
fn claude_settings_json(asylum_binary: &str) -> String {
    let bin = shell_quote(asylum_binary);
    let hook_cmd = format!("{bin} harness-event claude-hook");
    let statusline_cmd = format!("{bin} harness-event claude-statusline");

    // A single matcher-group forwarding one event to the bridge. `async_tool` marks
    // the PostToolUse group fire-and-forget so tool execution is never blocked. A
    // short 10s timeout keeps a slow/unreachable daemon from stalling the session
    // near claude's 600s hook default.
    let group = |async_tool: bool| -> Value {
        let mut entry = serde_json::Map::new();
        entry.insert("type".to_string(), Value::String("command".to_string()));
        entry.insert("command".to_string(), Value::String(hook_cmd.clone()));
        entry.insert("timeout".to_string(), Value::from(10));
        if async_tool {
            entry.insert("async".to_string(), Value::Bool(true));
        }
        serde_json::json!({ "hooks": [Value::Object(entry)] })
    };

    serde_json::json!({
        "hooks": {
            "Stop": [group(false)],
            "Notification": [group(false)],
            "SessionStart": [group(false)],
            "SessionEnd": [group(false)],
            "PostToolUse": [group(true)],
        },
        "statusLine": { "type": "command", "command": statusline_cmd }
    })
    .to_string()
}
