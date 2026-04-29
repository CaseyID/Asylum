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
