use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::relationship::RelationshipRecord;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeRecord {
    pub id: Uuid,
    pub harness: HarnessKind,
    pub substrate: SubstrateKind,
    pub role_hint: String,
    pub liveness: NodeLiveness,
    pub workspace: Option<String>,
    pub description: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub external_id: Option<String>,
    /// Harness-native session identity recorded from the harness-event bridge
    /// (claude `session_id`, codex `thread-id`). Resume key for Phase C.
    #[serde(default)]
    pub harness_session_id: Option<String>,
    /// Launch-profile model the node was actually launched with, recorded at launch
    /// time and passed through verbatim. `None` is the explicit harness-default
    /// marker (display layers render it as "harness default"). Survives
    /// restart/resume so historical nodes report the profile they ran under.
    #[serde(default)]
    pub model: Option<String>,
    /// Launch-profile reasoning effort the node was actually launched with. Same
    /// harness-default marker semantics as `model`.
    #[serde(default)]
    pub effort: Option<String>,
    pub capabilities: CapabilitySnapshot,
    #[serde(default)]
    pub tokens_in: u64,
    #[serde(default)]
    pub tokens_out: u64,
    #[serde(default)]
    pub tool_calls: u64,
    #[serde(default)]
    pub ctx_pct: f32,
    #[serde(default)]
    pub idle_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    Codex,
    ClaudeCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SubstrateKind {
    Local,
    Loon,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeLiveness {
    Starting,
    Running,
    WaitingForInput,
    Exited,
    Stopped,
    Failed,
    Archived,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CapabilitySnapshot {
    #[serde(default)]
    pub browser_attach: bool,
    #[serde(default)]
    pub native_attach: bool,
    #[serde(default)]
    pub send_input: bool,
    #[serde(default)]
    pub interrupt: bool,
    #[serde(default)]
    pub stop: bool,
    #[serde(default)]
    pub resume: bool,
    #[serde(default)]
    pub structured_events: bool,
    #[serde(default)]
    pub transcript_export: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphRecord {
    pub nodes: Vec<NodeRecord>,
    pub relationships: Vec<RelationshipRecord>,
}

impl Default for NodeRecord {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            harness: HarnessKind::Codex,
            substrate: SubstrateKind::Local,
            role_hint: "node".to_string(),
            liveness: NodeLiveness::Stopped,
            workspace: None,
            description: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            external_id: None,
            harness_session_id: None,
            model: None,
            effort: None,
            capabilities: CapabilitySnapshot::default(),
            tokens_in: 0,
            tokens_out: 0,
            tool_calls: 0,
            ctx_pct: 0.0,
            idle_seconds: 0,
        }
    }
}

impl NodeRecord {
    pub fn is_running_like(&self) -> bool {
        matches!(
            self.liveness,
            NodeLiveness::Running | NodeLiveness::WaitingForInput | NodeLiveness::Starting
        )
    }

    pub fn to_summary(&self) -> String {
        format!(
            "{} {} node {} on {}",
            self.harness, self.substrate, self.id, self.role_hint
        )
    }
}

impl std::fmt::Display for HarnessKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HarnessKind::Codex => write!(f, "codex"),
            HarnessKind::ClaudeCode => write!(f, "claude_code"),
        }
    }
}

impl std::fmt::Display for SubstrateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubstrateKind::Local => write!(f, "local"),
            SubstrateKind::Loon => write!(f, "loon"),
        }
    }
}

impl std::fmt::Display for NodeLiveness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeLiveness::Starting => write!(f, "starting"),
            NodeLiveness::Running => write!(f, "running"),
            NodeLiveness::WaitingForInput => write!(f, "waiting_for_input"),
            NodeLiveness::Exited => write!(f, "exited"),
            NodeLiveness::Stopped => write!(f, "stopped"),
            NodeLiveness::Failed => write!(f, "failed"),
            NodeLiveness::Archived => write!(f, "archived"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum NodeParseError {
    UnknownHarness(String),
    UnknownSubstrate(String),
}

impl std::fmt::Display for NodeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeParseError::UnknownHarness(value) => {
                write!(f, "unknown harness: {value}")
            }
            NodeParseError::UnknownSubstrate(value) => {
                write!(f, "unknown substrate: {value}")
            }
        }
    }
}

impl std::str::FromStr for HarnessKind {
    type Err = NodeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" | "claude_code" | "claudecode" => Ok(Self::ClaudeCode),
            other => Err(NodeParseError::UnknownHarness(other.to_string())),
        }
    }
}

impl std::str::FromStr for SubstrateKind {
    type Err = NodeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "loon" => Ok(Self::Loon),
            other => Err(NodeParseError::UnknownSubstrate(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_record_uses_snake_case_wire_values() {
        let node = NodeRecord {
            id: Uuid::nil(),
            harness: HarnessKind::ClaudeCode,
            substrate: SubstrateKind::Local,
            role_hint: "command-center".to_string(),
            liveness: NodeLiveness::WaitingForInput,
            workspace: Some("/tmp/asylum-demo".to_string()),
            description: "Main command center".to_string(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            external_id: None,
            harness_session_id: Some("sess-1".to_string()),
            model: Some("opus".to_string()),
            effort: Some("high".to_string()),
            capabilities: CapabilitySnapshot {
                browser_attach: true,
                native_attach: true,
                send_input: true,
                interrupt: true,
                stop: true,
                resume: false,
                structured_events: false,
                transcript_export: false,
            },
            tokens_in: 12,
            tokens_out: 34,
            tool_calls: 5,
            ctx_pct: 0.25,
            idle_seconds: 7,
        };

        let value = serde_json::to_value(&node).unwrap();
        assert_eq!(value["harness"], "claude_code");
        assert_eq!(value["substrate"], "local");
        assert_eq!(value["liveness"], "waiting_for_input");
        assert_eq!(value["tokens_in"], 12);
        assert_eq!(value["tokens_out"], 34);
        assert_eq!(value["tool_calls"], 5);
        assert_eq!(value["idle_seconds"], 7);
        assert!(value["ctx_pct"].is_number());
        assert_eq!(value["model"], "opus");
        assert_eq!(value["effort"], "high");

        let round_trip: NodeRecord = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.tokens_in, 12);
        assert_eq!(round_trip.tokens_out, 34);
        assert_eq!(round_trip.tool_calls, 5);
        assert_eq!(round_trip.idle_seconds, 7);
        assert_eq!(round_trip.model.as_deref(), Some("opus"));
        assert_eq!(round_trip.effort.as_deref(), Some("high"));
    }

    #[test]
    fn node_record_profile_defaults_to_harness_default_marker() {
        // A payload from before launch profiles existed omits model/effort; both
        // decode to the None harness-default marker.
        let legacy = serde_json::json!({
            "id": Uuid::nil(),
            "harness": "codex",
            "substrate": "local",
            "role_hint": "worker",
            "liveness": "running",
            "workspace": null,
            "description": "",
            "created_at": "1970-01-01T00:00:00Z",
            "updated_at": "1970-01-01T00:00:00Z",
            "external_id": null,
            "capabilities": {},
        });
        let node: NodeRecord = serde_json::from_value(legacy).unwrap();
        assert_eq!(node.model, None);
        assert_eq!(node.effort, None);
    }

    // H4 fix: CapabilitySnapshot must tolerate JSON that is missing some or all fields,
    // so that adding new capability flags does not break existing stored rows.

    #[test]
    fn capability_snapshot_partial_json_defaults_missing_fields_to_false() {
        // Only browser_attach and send_input are present; all others must default.
        let json = r#"{"browser_attach": true, "send_input": true}"#;
        let snap: CapabilitySnapshot = serde_json::from_str(json).unwrap();
        assert!(snap.browser_attach);
        assert!(snap.send_input);
        assert!(!snap.native_attach);
        assert!(!snap.interrupt);
        assert!(!snap.stop);
        assert!(!snap.resume);
        assert!(!snap.structured_events);
        assert!(!snap.transcript_export);
    }

    #[test]
    fn capability_snapshot_empty_object_deserializes_to_all_defaults() {
        let snap: CapabilitySnapshot = serde_json::from_str("{}").unwrap();
        assert_eq!(snap, CapabilitySnapshot::default());
        assert!(!snap.browser_attach);
        assert!(!snap.native_attach);
        assert!(!snap.send_input);
        assert!(!snap.interrupt);
        assert!(!snap.stop);
        assert!(!snap.resume);
        assert!(!snap.structured_events);
        assert!(!snap.transcript_export);
    }

    #[test]
    fn capability_snapshot_full_roundtrip() {
        let original = CapabilitySnapshot {
            browser_attach: true,
            native_attach: true,
            send_input: true,
            interrupt: true,
            stop: true,
            resume: false,
            structured_events: false,
            transcript_export: false,
        };
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: CapabilitySnapshot = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
        // Verify the JSON contains the expected keys.
        let v: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(v["browser_attach"], true);
        assert_eq!(v["native_attach"], true);
        assert_eq!(v["send_input"], true);
        assert_eq!(v["interrupt"], true);
        assert_eq!(v["stop"], true);
        assert_eq!(v["resume"], false);
        assert_eq!(v["structured_events"], false);
        assert_eq!(v["transcript_export"], false);
    }
}
