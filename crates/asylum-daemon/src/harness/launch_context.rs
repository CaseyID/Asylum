use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct LaunchContext {
    pub node_id: Uuid,
    pub workspace: Option<PathBuf>,
    pub role_hint: String,
    pub graph_summary: String,
    pub capabilities: Vec<String>,
}

impl LaunchContext {
    pub fn new(node_id: Uuid, role_hint: String, workspace: Option<PathBuf>) -> Self {
        Self {
            node_id,
            workspace,
            role_hint,
            graph_summary: "No peers in graph yet.".to_string(),
            capabilities: Vec::new(),
        }
    }

    pub fn instruction_prompt(&self) -> String {
        let workspace_display = self
            .workspace
            .as_deref()
            .map_or_else(|| "<none>".to_string(), |path| path.display().to_string());
        format!(
            "You are node {} with role '{}'.\nWorkspace: {}\nCapabilities: {:?}\nSystem map: {}\n\nAsylum control: use the configured Asylum MCP tools for node and graph operations. To create or supervise other Asylum nodes, call tools such as node.spawn_peer, node.create, graph.get, relationship.create, and node.send_input. Do not simulate worker nodes inside your own harness session.",
            self.node_id,
            self.role_hint,
            workspace_display,
            self.capabilities.join(", "),
            self.graph_summary,
        )
    }
}
