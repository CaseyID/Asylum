use asylum_core::node::CapabilitySnapshot;
use asylum_core::node::HarnessKind;

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

    fn launch_context(&self, request: &asylum_core::api::CreateNodeRequest) -> String {
        let context = LaunchContext {
            node_id: uuid::Uuid::new_v4(),
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
}
