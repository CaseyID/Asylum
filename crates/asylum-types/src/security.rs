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

/// Advisory token scope enum (M21). In v1 (single-user), the daemon grants
/// full access regardless of scope — this type is published for client-side
/// documentation and future enforcement. Wiring scope into auth validation
/// is tracked as a follow-up for multi-user/team support.
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
    pub fn is_expired_at(&self, now_epoch_secs: i64) -> bool {
        self.expires_at_epoch_secs <= now_epoch_secs
    }
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
    /// Advisory labels only -- not enforced per-route (see `TokenScope` above).
    pub scope: Vec<String>,
    pub ttl_seconds: Option<u64>,
}

impl TokenRequest {
    pub fn ttl_or_default(&self) -> Duration {
        Duration::from_secs(self.ttl_seconds.unwrap_or(3600))
    }
}
