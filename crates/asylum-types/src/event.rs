use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeEvent {
    pub id: i64,
    pub node_id: Uuid,
    pub sequence: i64,
    pub kind: NodeEventKind,
    pub body: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Schema version for this event record. Defaults to 1 for all existing
    /// and new events. Used for forward-compatibility detection when replaying
    /// historical events.
    /// TODO: replace with a tagged-body enum per kind when body formats stabilise.
    #[serde(default = "NodeEvent::default_schema_version")]
    pub schema_version: u32,
}

impl NodeEvent {
    pub fn default_schema_version() -> u32 {
        1
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeEventKind {
    NodeStarted,
    OutputChunk,
    InputSent,
    LivenessChanged,
    HarnessFailure,
    SubstrateFailure,
    HumanInputRequested,
    NotificationSent,
    RemoteCommandReceived,
    AttachIssued,
}
