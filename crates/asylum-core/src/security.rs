use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IssuedToken {
    pub id: Uuid,
    pub owner_name: String,
    pub raw_token: String,
    pub stored_hash: String,
    pub scope: Vec<TokenScope>,
    pub expires_at_epoch_secs: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    TokenIssue,
    TokenRevoke,
    NodeCreate,
    NodeList,
    NodeInspect,
    NodeInput,
    NodeInterrupt,
    NodeStop,
    NodeArchive,
    NodeObserve,
    NodeSendInput,
    NodeAttachBrowser,
    NodeAttachNativeTarget,
    RelationshipList,
    GraphGet,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttachToken {
    pub raw: String,
    pub node_id: Uuid,
    pub expires_at_epoch_secs: i64,
}

impl AttachToken {
    pub fn is_expired(&self) -> bool {
        let now = chrono_like_unix_now();
        self.expires_at_epoch_secs <= now
    }
}

pub fn chrono_like_unix_now() -> i64 {
    let duration = time::OffsetDateTime::now_utc() - time::OffsetDateTime::UNIX_EPOCH;
    duration.whole_seconds()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenVerification {
    pub token_id: Uuid,
    pub owner_name: String,
    pub scopes: Vec<TokenScope>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenRequest {
    pub name: String,
    pub scope: Vec<String>,
    pub ttl_seconds: Option<u64>,
}

impl TokenRequest {
    pub fn ttl_or_default(&self) -> Duration {
        Duration::from_secs(self.ttl_seconds.unwrap_or(3600))
    }
}
