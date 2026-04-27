use serde::{Deserialize, Serialize};

use crate::capabilities::CapabilityDescriptor;
use crate::event::NodeEvent;
use crate::node::{GraphRecord, NodeLiveness, NodeRecord};
use crate::relationship::RelationshipRecord;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateNodeRequest {
    pub harness: String,
    pub substrate: String,
    pub role_hint: String,
    pub workspace: Option<String>,
    pub description: Option<String>,
    pub created_by: Option<String>,
    #[serde(default)]
    pub launch_args: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeCreateResponse {
    pub node_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInspectResponse {
    pub node: NodeRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeListResponse {
    pub nodes: Vec<NodeRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeEventsResponse {
    pub events: Vec<NodeEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendInputRequest {
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttachRequest {
    pub include_input: bool,
    pub include_stdout: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttachResponse {
    pub url: String,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeAttachResponse {
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub environment: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarnessListResponse {
    pub items: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubstrateListResponse {
    pub items: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapabilityListResponse {
    pub capabilities: Vec<CapabilityDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphGetResponse {
    pub graph: crate::node::GraphRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationshipCreateRequest {
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: String,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationshipDeleteRequest {
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationshipResponse {
    pub relationships: Vec<RelationshipRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapabilityCheck {
    pub capability: String,
    pub enabled: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubstrateHealth {
    pub status: String,
    pub running_instances: usize,
    pub harness_profiles: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notification {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub node_id: Option<String>,
    pub created_at_epoch_secs: i64,
    pub read_at_epoch_secs: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotificationsResponse {
    pub notifications: Vec<Notification>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientConfigResponse {
    pub base_url: String,
    pub capabilities_endpoint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchPacketResponse {
    pub markdown: String,
    pub artifact_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeCapability {
    pub node_id: String,
    pub capability: String,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListCapabilitiesResponse {
    pub harness_caps: Vec<NodeCapability>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenIssueRequest {
    pub name: String,
    pub scope: Vec<String>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenIssueResponse {
    pub id: String,
    pub raw_token: String,
    pub scope: Vec<String>,
    pub expires_at_epoch_secs: i64,
}

pub fn map_graph(nodes: Vec<NodeRecord>, relationships: Vec<RelationshipRecord>) -> GraphRecord {
    GraphRecord {
        nodes,
        relationships,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteCommandResponse {
    pub kind: String,
    pub status: String,
    pub node_id: Option<String>,
    pub result: serde_json::Value,
}

impl NodeLiveness {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            NodeLiveness::Exited
                | NodeLiveness::Stopped
                | NodeLiveness::Failed
                | NodeLiveness::Archived
        )
    }

    pub fn in_progress(&self) -> bool {
        !self.is_terminal()
    }
}
