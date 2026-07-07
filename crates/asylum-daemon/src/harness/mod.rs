use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::harness::claude::ClaudeHarness;
use crate::harness::codex::CodexHarness;
use asylum_types::config::HarnessConfig;
use asylum_types::node::{CapabilitySnapshot, HarnessKind};
use uuid::Uuid;

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
    fn launch_context(
        &self,
        node_id: Uuid,
        request: &asylum_types::api::CreateNodeRequest,
    ) -> String;
    fn asylum_control_args(
        &self,
        _asylum_binary: &str,
        _socket_path: Option<&str>,
        _node_id: Uuid,
        _session_id: Option<Uuid>,
    ) -> Vec<String> {
        Vec::new()
    }

    /// A harness session id pre-assigned at node-create time so the daemon knows
    /// the resume key up front (claude `--session-id <uuid>`). Returns `None` for
    /// harnesses whose session id can only be discovered after launch (codex --
    /// recorded from the first notify post). Called once per launch; the returned
    /// id is both stored on the node row and threaded into `asylum_control_args`.
    fn preassign_session_id(&self) -> Option<Uuid> {
        None
    }
    /// Idempotently record the workspace path as trusted in the harness's own config
    /// so the first-run trust dialog is skipped when the process spawns.
    fn pre_trust_workspace(&self, workspace: &str) -> anyhow::Result<()>;

    /// Whether the harness reports idleness natively (claude via the Notification
    /// `idle_prompt` hook). Harnesses that return false rely on the daemon's
    /// output-quiescence timer for `node.idle` (codex).
    fn native_idle_signal(&self) -> bool {
        false
    }
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
        config.startup_args.insert(
            "codex".to_string(),
            vec!["--model".to_string(), "o3".to_string()],
        );

        let registry = HarnessRegistry::from_config(&config);
        let codex = registry.get(&HarnessKind::Codex).unwrap();
        let args = codex.launch_args();

        // Default flag must come first
        assert_eq!(args[0], "--dangerously-bypass-approvals-and-sandbox");
        // Config-supplied args follow
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"o3".to_string()));
    }

    #[test]
    fn codex_control_args_register_asylum_mcp_per_launch() {
        let registry = HarnessRegistry::default();
        let codex = registry.get(&HarnessKind::Codex).unwrap();
        let node_id = Uuid::new_v4();

        let args = codex.asylum_control_args(
            "/usr/local/bin/asylum",
            Some("/tmp/asylum.sock"),
            node_id,
            None,
        );
        let joined = args.join("\n");

        assert!(joined.contains("mcp_servers.asylum.command=\"/usr/local/bin/asylum\""));
        assert!(joined.contains("mcp_servers.asylum.args=[\"mcp\"]"));
        assert!(joined.contains(&format!("ASYLUM_NODE_ID=\"{}\"", node_id)));
        assert!(joined.contains("ASYLUM_SOCKET_PATH=\"/tmp/asylum.sock\""));
        assert!(joined.contains("mcp_servers.asylum.required=true"));
    }

    #[test]
    fn codex_control_args_route_notify_through_bridge() {
        let registry = HarnessRegistry::default();
        let codex = registry.get(&HarnessKind::Codex).unwrap();
        let node_id = Uuid::new_v4();

        let args = codex.asylum_control_args(
            "/usr/local/bin/asylum",
            Some("/tmp/asylum.sock"),
            node_id,
            // Codex ignores a pre-assigned session id (no --session-id equivalent).
            Some(Uuid::new_v4()),
        );

        // The notify value is a `-c` override whose value is a TOML array of strings.
        let notify_pos = args
            .iter()
            .position(|arg| arg.starts_with("notify="))
            .expect("codex launch args should include a notify override");
        assert_eq!(args[notify_pos - 1], "-c");
        assert_eq!(
            args[notify_pos],
            "notify=[\"/usr/local/bin/asylum\",\"harness-event\",\"codex-notify\"]"
        );

        // No --session-id / --settings leaks into the codex argv.
        assert!(!args.iter().any(|a| a == "--session-id"));
        assert!(!args.iter().any(|a| a == "--settings"));
    }

    #[test]
    fn claude_control_args_register_asylum_mcp_per_launch() -> anyhow::Result<()> {
        let registry = HarnessRegistry::default();
        let claude = registry.get(&HarnessKind::ClaudeCode).unwrap();
        let node_id = Uuid::new_v4();

        let session_id = Uuid::new_v4();
        let args = claude.asylum_control_args(
            "/opt/asylum/bin/asylum",
            Some("/tmp/asylum.sock"),
            node_id,
            Some(session_id),
        );
        let config_index = args
            .iter()
            .position(|arg| arg == "--mcp-config")
            .expect("claude launch args should include --mcp-config");
        let config: serde_json::Value = serde_json::from_str(&args[config_index + 1])?;

        assert_eq!(
            config["mcpServers"]["asylum"]["command"],
            "/opt/asylum/bin/asylum"
        );
        assert_eq!(config["mcpServers"]["asylum"]["args"][0], "mcp");
        assert_eq!(
            config["mcpServers"]["asylum"]["env"]["ASYLUM_NODE_ID"],
            node_id.to_string()
        );
        assert_eq!(
            config["mcpServers"]["asylum"]["env"]["ASYLUM_SOCKET_PATH"],
            "/tmp/asylum.sock"
        );
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        assert!(args.contains(&"--allowedTools".to_string()));
        assert!(args.contains(&"mcp__asylum__*".to_string()));

        // Pre-assigned session id is passed as `--session-id <uuid>`.
        let sid_index = args
            .iter()
            .position(|arg| arg == "--session-id")
            .expect("claude launch args should include --session-id");
        assert_eq!(args[sid_index + 1], session_id.to_string());

        // Reporting hooks + statusline are injected inline via --settings.
        let settings_index = args
            .iter()
            .position(|arg| arg == "--settings")
            .expect("claude launch args should include --settings");
        let settings: serde_json::Value = serde_json::from_str(&args[settings_index + 1])?;

        // statusLine invokes the claude-statusline bridge.
        assert_eq!(settings["statusLine"]["type"], "command");
        assert_eq!(
            settings["statusLine"]["command"],
            "'/opt/asylum/bin/asylum' harness-event claude-statusline"
        );

        // Every reporting hook fires the claude-hook bridge. Stop/Notification/
        // SessionStart/SessionEnd are blocking with a short timeout; PostToolUse is
        // async so it never blocks tool execution. No matchers -> match all.
        let hooks = &settings["hooks"];
        for event in ["Stop", "Notification", "SessionStart", "SessionEnd"] {
            let group = &hooks[event][0];
            assert!(group.get("matcher").is_none(), "{event} must not set a matcher");
            let entry = &group["hooks"][0];
            assert_eq!(entry["type"], "command");
            assert_eq!(
                entry["command"],
                "'/opt/asylum/bin/asylum' harness-event claude-hook"
            );
            assert_eq!(entry["timeout"], 10);
            assert!(
                entry.get("async").is_none(),
                "{event} reporting hook must be blocking"
            );
        }
        let post = &hooks["PostToolUse"][0];
        assert!(post.get("matcher").is_none(), "PostToolUse must match all tools");
        let post_entry = &post["hooks"][0];
        assert_eq!(
            post_entry["command"],
            "'/opt/asylum/bin/asylum' harness-event claude-hook"
        );
        assert_eq!(post_entry["timeout"], 10);
        assert_eq!(post_entry["async"], true);

        Ok(())
    }

    #[test]
    fn only_claude_preassigns_a_session_id() {
        let registry = HarnessRegistry::default();
        let claude = registry.get(&HarnessKind::ClaudeCode).unwrap();
        let codex = registry.get(&HarnessKind::Codex).unwrap();

        // claude pre-assigns a fresh uuid per launch; codex has no pre-assignable id.
        assert!(claude.preassign_session_id().is_some());
        assert_ne!(claude.preassign_session_id(), claude.preassign_session_id());
        assert!(codex.preassign_session_id().is_none());
    }

    #[test]
    fn claude_control_args_keep_skip_permissions_leading_and_append_user_args() {
        // Mirror create_node's assembly order: launch_args() (leading trust-bypass
        // flag + config startup_args) ++ control args ++ per-request launch_args.
        let registry = HarnessRegistry::default();
        let claude = registry.get(&HarnessKind::ClaudeCode).unwrap();
        let node_id = Uuid::new_v4();

        let mut argv = claude.launch_args().to_vec();
        argv.extend(claude.asylum_control_args(
            "/opt/asylum/bin/asylum",
            Some("/tmp/asylum.sock"),
            node_id,
            Some(Uuid::new_v4()),
        ));
        argv.extend(["--model".to_string(), "opus".to_string()]);

        // Documented routing quirk: --dangerously-skip-permissions must stay leading.
        assert_eq!(argv[0], "--dangerously-skip-permissions");
        // Control args land before the user-supplied trailing args.
        let settings_pos = argv.iter().position(|a| a == "--settings").unwrap();
        let model_pos = argv.iter().position(|a| a == "--model").unwrap();
        assert!(settings_pos < model_pos);
        assert_eq!(argv.last().unwrap(), "opus");
    }
}
