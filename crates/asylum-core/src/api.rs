use serde::{Deserialize, Serialize};

use crate::capabilities::CapabilityDescriptor;
use crate::event::NodeEvent;
use crate::node::{GraphRecord, NodeLiveness, NodeRecord};
use crate::relationship::RelationshipRecord;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub daemon_version: String,
    pub bind_addr: String,
    pub database_path: String,
    pub database_size_bytes: u64,
    pub transcripts_dir: String,
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
pub struct HarnessDescriptor {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub available: bool,
    pub command: String,
    pub caps: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarnessDescriptorResponse {
    pub harnesses: Vec<HarnessDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubstrateDescriptor {
    pub id: String,
    pub name: String,
    pub host: String,
    pub healthy: bool,
    pub capacity: f32,
    pub nodes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubstrateDescriptorResponse {
    pub substrates: Vec<SubstrateDescriptor>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenSummary {
    pub id: String,
    pub label: String,
    pub created_at_epoch_secs: i64,
    pub expires_at_epoch_secs: i64,
    pub revoked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenListResponse {
    pub tokens: Vec<TokenSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenRotateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenRotateResponse {
    pub old_id: String,
    pub new_token: TokenIssueResponse,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelDescriptor {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub label: String,
    pub direction: String,
    pub status: String,
    pub detail: String,
    pub config: serde_json::Value,
    pub live: bool,
    pub builtin: bool,
    pub created_at_epoch_secs: i64,
    pub message_count_24h: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelMessageRecord {
    pub id: i64,
    pub channel_id: String,
    pub direction: String,
    pub ts_epoch_secs: i64,
    pub sender: String,
    pub subject: String,
    pub body: String,
    pub replies: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelListResponse {
    pub channels: Vec<ChannelDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelMessagesResponse {
    pub messages: Vec<ChannelMessageRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelTestRequest {
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelTestResponse {
    pub sent: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelCreateRequest {
    pub kind: String,
    pub name: String,
    pub label: Option<String>,
    pub direction: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub live: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelUpdateRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub live: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelInboundRequest {
    pub sender: String,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub replies: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookAction {
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub event: String,
    pub filter: String,
    pub actions: Vec<HookAction>,
    pub future: bool,
    pub created_at_epoch_secs: i64,
    pub updated_at_epoch_secs: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookCreateRequest {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub event: String,
    #[serde(default = "default_filter_any")]
    pub filter: String,
    #[serde(default)]
    pub actions: Vec<HookAction>,
    #[serde(default)]
    pub future: bool,
}

fn default_true() -> bool {
    true
}

fn default_filter_any() -> String {
    "any".to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HookUpdateRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub actions: Option<Vec<HookAction>>,
    #[serde(default)]
    pub future: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookListResponse {
    pub hooks: Vec<HookRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookFiringRecord {
    pub id: i64,
    pub hook_id: String,
    pub ts_epoch_secs: i64,
    pub trigger: String,
    pub outcome: String,
    pub ok: bool,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookFiringsResponse {
    pub firings: Vec<HookFiringRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookEventCatalogEntry {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookEventCatalogResponse {
    pub events: Vec<HookEventCatalogEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookTestResponse {
    pub firing: HookFiringRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecipeDescriptor {
    pub id: String,
    pub title: String,
    pub prompt_template: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecipeListResponse {
    pub recipes: Vec<RecipeDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecipeSpawnRequest {
    pub harness: String,
    pub substrate: String,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub role_hint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecipeSpawnResponse {
    pub node_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForkNodeRequest {
    #[serde(default)]
    pub role_hint: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
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
