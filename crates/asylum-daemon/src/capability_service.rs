use std::{
    collections::HashMap,
    env,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use asylum_types::api::{
    AttachResponse, CapabilityListResponse, ClientConfigResponse, CreateNodeRequest,
    DecisionCreateRequest, DecisionListResponse, DecisionRecord, DecisionResolveRequest,
    GraphGetResponse, HarnessDescriptor, HarnessDescriptorResponse, HarnessEventRequest,
    HarnessEventResponse, HarnessListResponse, HealthResponse, LaunchPacketResponse,
    NativeAttachResponse, NodeCreateResponse, NodeEventsResponse, NodeInspectResponse,
    NodeListResponse, Notification, NotificationsResponse, RelationshipCreateRequest,
    RelationshipResponse, RemoteCommandResponse, SendInputRequest, SpawnPeerRequest,
    SpawnPeerResponse, SubstrateDescriptor, SubstrateDescriptorResponse, SubstrateHealth,
    SubstrateListResponse, TokenIssueResponse,
};
use asylum_types::capabilities::CapabilityDescriptor;
use asylum_types::capabilities::CapabilityName;
use asylum_types::event::NodeEventKind;
use asylum_types::node::{CapabilitySnapshot, HarnessKind, NodeLiveness, SubstrateKind};
use asylum_types::relationship::RelationshipKind;
use asylum_types::security::TokenRequest;
use serde_json::{json, Value as JsonValue};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::attach::AttachTokenIssuer;
use crate::auth::{issue_owner_token, AuthMode};
use crate::channels::ntfy_inbound::NtfyInboundConfig;
use crate::channels::{
    descriptor_from_row, is_implemented_channel_kind, message_record_from_row, ntfy_inbound,
    render_template, require_channel, seed_builtin_channels, SeedConfig, NTFY_DEFAULT_ID,
};
use crate::decision_ingester::{DecisionProtocolRequest, ASYLUM_DECISION_PROTOCOL};
use crate::harness::HarnessRegistry;
use crate::hooks::{
    evaluate_filter, event_catalog, firing_record_from_row, rule_from_row, HookEngine, HookEvent,
    SCHEDULE_30M, SCHEDULE_5M,
};
use crate::notifications::send_with_optional_config;
use crate::recipes;
use crate::remote_commands::{parse_remote_command, ParsedRemoteCommand, RemoteCommandKind};
use crate::storage::Store;
use crate::substrate::loon::{capability_flags_from_health, LoonHealth, LoonSubstrate};
use crate::substrate::{ExitOutcome, LocalSubstrate, SubstrateContext};
use asylum_types::api::{
    ChannelCreateRequest, ChannelDescriptor, ChannelInboundRequest, ChannelListResponse,
    ChannelMessagesResponse, ChannelTestRequest, ChannelTestResponse, ChannelUpdateRequest,
    ForkNodeRequest, HookAction, HookCreateRequest, HookEventCatalogResponse, HookFiringsResponse,
    HookListResponse, HookRule, HookTestResponse, HookUpdateRequest, RecipeListResponse,
    RecipeSpawnRequest, RecipeSpawnResponse,
};
use asylum_types::config::{AutonomyConfig, HarnessConfig, LoonConfig};
use asylum_types::node::NodeRecord;

const CHANNEL_REPLY_TOKEN_LENGTH: usize = 5;
const CHANNEL_REPLY_CORRELATION_TTL_SECONDS: i64 = 60 * 30;
// Awaiting-input / permission-prompt decisions surface under the single
// `node.awaiting_input` catalog event (permission_requested was merged in).
const NODE_AWAITING_INPUT_HOOK_EVENT: &str = "node.awaiting_input";

#[derive(Clone)]
struct LocalDecisionIngestion {
    store: Store,
    hook_engine: Arc<HookEngine>,
}

impl LocalDecisionIngestion {
    fn ingest_request(
        &self,
        node_id: Uuid,
        request: DecisionProtocolRequest,
    ) -> Result<DecisionRecord> {
        let source = request.source;
        let actions = request.actions;
        let node = self.store.get_node(node_id)?.context("node not found")?;
        if request.text.trim().is_empty() {
            return Err(anyhow!("decision text required"));
        }
        let decision = map_decision(
            self.store
                .insert_decision(Some(node.id), request.text.trim())?,
        );
        let _ = self.store.insert_notification(
            Some(node.id),
            "decision",
            "Decision requested",
            &decision.text,
        );
        let _ = self.store.record_event(
            node.id,
            NodeEventKind::HumanInputRequested,
            json!({
                "decision": decision.id,
                "text": decision.text,
                "source": source.clone(),
                "actions": actions.clone(),
            }),
        );
        if let Err(e) = self
            .store
            .set_node_liveness(node.id, NodeLiveness::WaitingForInput)
        {
            tracing::warn!(error = %e, node_id = %node.id, "failed to set node waiting_for_input");
        }
        if hook_event_is_supported() {
            self.hook_engine.post(HookEvent {
                event: NODE_AWAITING_INPUT_HOOK_EVENT.to_string(),
                node_id: Some(node.id),
                payload: json!({
                    "decision": decision.id,
                    "node": {"id": node.id.to_string()},
                    "type": "permission_prompt",
                    "source": source,
                    "actions": actions,
                }),
            });
        }
        Ok(decision)
    }
}

fn hook_event_is_supported() -> bool {
    event_catalog()
        .iter()
        .any(|event| event.id == NODE_AWAITING_INPUT_HOOK_EVENT)
}

/// A live node still owns its process and can accept harness signals; terminal
/// nodes are never resurrected by an ingested event.
fn is_active_liveness(liveness: &NodeLiveness) -> bool {
    matches!(
        liveness,
        NodeLiveness::Starting | NodeLiveness::Running | NodeLiveness::WaitingForInput
    )
}

/// The result of mapping a raw harness payload to Asylum's event model. All
/// interpretation lives here (daemon-side) so the CLI bridge stays thin.
#[derive(Default)]
struct MappedHarnessEvent {
    /// Catalog event kind to store + fire, if any.
    event: Option<&'static str>,
    /// Liveness the node should move to as a result, if the event implies one.
    liveness: Option<NodeLiveness>,
    /// Extra fields merged into the stored and posted payload.
    detail: JsonValue,
    /// Harness session id carried by the payload (claude `session_id`,
    /// codex `thread-id`), recorded on the node row.
    session_id: Option<String>,
    /// Statusline posts are handled via the telemetry/ctx_pressure path rather
    /// than a single direct catalog event.
    telemetry: bool,
}

fn payload_str(payload: &JsonValue, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn truncate_for_detail(value: &JsonValue, max: usize) -> JsonValue {
    let rendered = match value {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    };
    if rendered.chars().count() <= max {
        json!(rendered)
    } else {
        let truncated: String = rendered.chars().take(max).collect();
        json!(format!("{truncated}…"))
    }
}

/// Map `(source, payload)` to Asylum's event model. Pure and unit-tested.
fn map_harness_event(source: &str, payload: &JsonValue) -> MappedHarnessEvent {
    let mut mapped = MappedHarnessEvent::default();
    match source {
        "claude_hook" => {
            mapped.session_id = payload_str(payload, "session_id");
            let hook = payload
                .get("hook_event_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            match hook {
                "Stop" => {
                    mapped.event = Some("node.turn_complete");
                    mapped.liveness = Some(NodeLiveness::Running);
                }
                "SessionStart" => {
                    mapped.event = Some("node.session_started");
                    mapped.liveness = Some(NodeLiveness::Running);
                    if let Some(src) = payload.get("source") {
                        mapped.detail = json!({ "source": src });
                    }
                }
                "SessionEnd" => {
                    mapped.event = Some("node.session_end");
                    if let Some(reason) = payload.get("reason") {
                        mapped.detail = json!({ "reason": reason });
                    }
                }
                "PostToolUse" => {
                    mapped.event = Some("node.tool_call");
                    mapped.liveness = Some(NodeLiveness::Running);
                    mapped.detail = json!({
                        "tool_name": payload.get("tool_name").cloned().unwrap_or(JsonValue::Null),
                        "tool_input": payload
                            .get("tool_input")
                            .map(|v| truncate_for_detail(v, 200))
                            .unwrap_or(JsonValue::Null),
                    });
                }
                "Notification" => {
                    let ntype = payload
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let message = payload.get("message").cloned().unwrap_or(JsonValue::Null);
                    match ntype {
                        "permission_prompt" | "agent_needs_input" => {
                            mapped.event = Some("node.awaiting_input");
                            mapped.liveness = Some(NodeLiveness::WaitingForInput);
                            mapped.detail = json!({ "type": ntype, "message": message });
                        }
                        "idle_prompt" => {
                            mapped.event = Some("node.idle");
                            mapped.liveness = Some(NodeLiveness::Running);
                            mapped.detail = json!({ "type": ntype, "message": message, "idle_source": "notification" });
                        }
                        "agent_completed" => {
                            mapped.event = Some("node.turn_complete");
                            mapped.liveness = Some(NodeLiveness::Running);
                        }
                        other if other.starts_with("elicitation") => {
                            mapped.event = Some("node.awaiting_input");
                            mapped.liveness = Some(NodeLiveness::WaitingForInput);
                            mapped.detail = json!({ "type": other, "message": message });
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        "codex_notify" => {
            mapped.session_id = payload_str(payload, "thread-id");
            let ntype = payload
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if ntype == "agent-turn-complete" {
                mapped.event = Some("node.turn_complete");
                mapped.liveness = Some(NodeLiveness::Running);
                mapped.detail = json!({
                    "last_assistant_message": payload
                        .get("last-assistant-message")
                        .cloned()
                        .unwrap_or(JsonValue::Null),
                    "turn_id": payload.get("turn-id").cloned().unwrap_or(JsonValue::Null),
                });
            }
        }
        "claude_statusline" => {
            mapped.telemetry = true;
            mapped.session_id = payload_str(payload, "session_id");
        }
        _ => {}
    }
    mapped
}

#[derive(Clone)]
pub struct AppConfig {
    pub base_url: String,
    pub bind_addr: String,
    pub socket_path: Option<String>,
    pub transcripts_dir: String,
    pub workspace_recent_limit: usize,
    pub ntfy_server: Option<String>,
    pub ntfy_topic: Option<String>,
    pub ntfy_token: Option<String>,
    pub ntfy_poll_interval_seconds: Option<u64>,
    pub harness: HarnessConfig,
    pub loon: LoonConfig,
    pub autonomy: AutonomyConfig,
}

#[derive(Clone)]
pub struct CapabilityService {
    pub store: Store,
    pub harnesses: HarnessRegistry,
    pub local_substrate: Arc<LocalSubstrate>,
    pub loon_substrate: Option<Arc<LoonSubstrate>>,
    pub auth_mode: AuthMode,
    attach_issuer: Arc<AttachTokenIssuer>,
    pub config: AppConfig,
    pub hook_engine: Arc<HookEngine>,
    /// Quiescence-timer dedup: last PTY-output epoch a `node.idle` was fired
    /// against, per node. Prevents repeat idle events until fresh output arrives.
    idle_fired: Arc<Mutex<HashMap<Uuid, i64>>>,
}

impl CapabilityService {
    pub fn new(store: Store, auth_mode: AuthMode, config: AppConfig) -> Self {
        let issuer = AttachTokenIssuer::new(
            std::env::var("ASYLUM_ATTACH_SECRET").unwrap_or_else(|_| Uuid::new_v4().to_string()),
        );
        let hook_engine = HookEngine::new();
        let sink_store = store.clone();
        let decision_ingester = LocalDecisionIngestion {
            store: store.clone(),
            hook_engine: hook_engine.clone(),
        };
        let exit_store = store.clone();
        let exit_engine = hook_engine.clone();
        let local_substrate = LocalSubstrate::new_with_sinks(
            move |node_id, chunk| {
                if let Err(e) = sink_store.append_transcript_chunk(node_id, chunk) {
                    tracing::warn!(error = %e, "failed to persist transcript chunk");
                }
            },
            move |node_id, request| {
                if let Err(e) = decision_ingester.ingest_request(node_id, request) {
                    tracing::warn!(error = %e, node_id = %node_id, "failed to ingest decision request");
                }
            },
            move |node_id, outcome: ExitOutcome| {
                let store = exit_store.clone();
                let engine = exit_engine.clone();
                tokio::runtime::Handle::current().spawn(async move {
                    // The exit sink is the sole owner of process-termination truth.
                    // Only act when the process died while the daemon still
                    // considered it live; user stop/archive already set a terminal
                    // liveness and posted node.exited, so those are left untouched.
                    if let Ok(Some(node)) = store.get_node(node_id) {
                        if matches!(
                            node.liveness,
                            NodeLiveness::Running
                                | NodeLiveness::Starting
                                | NodeLiveness::WaitingForInput
                        ) {
                            if outcome.success {
                                let _ = store.set_node_liveness(node_id, NodeLiveness::Stopped);
                                engine.post(HookEvent {
                                    event: "node.exited".to_string(),
                                    node_id: Some(node_id),
                                    payload: json!({
                                        "node": {"id": node_id.to_string()},
                                        "reason": "exited",
                                        "exit_code": outcome.code,
                                    }),
                                });
                            } else {
                                let _ = store.set_node_liveness(node_id, NodeLiveness::Failed);
                                engine.post(HookEvent {
                                    event: "node.errored".to_string(),
                                    node_id: Some(node_id),
                                    payload: json!({
                                        "node": {"id": node_id.to_string()},
                                        "reason": "abnormal_exit",
                                        "exit_code": outcome.code,
                                    }),
                                });
                            }
                        }
                    }
                });
            },
        );
        let loon_substrate = if config.loon.enabled {
            Some(Arc::new(LoonSubstrate::new(
                &config.loon.endpoint,
                config.loon.cli_path.clone(),
                config.loon.api_key_file.clone(),
                config.loon.cert_fingerprint_file.clone(),
                true,
            )))
        } else {
            None
        };
        if let Err(err) = seed_builtin_channels(
            &store,
            SeedConfig {
                ntfy_configured: config.ntfy_server.is_some() && config.ntfy_topic.is_some(),
            },
        ) {
            tracing::error!(error = %err, "failed to seed built-in channels at startup");
        }
        Self {
            store,
            harnesses: HarnessRegistry::from_config(&config.harness),
            local_substrate: Arc::new(local_substrate),
            loon_substrate,
            auth_mode,
            attach_issuer: Arc::new(issuer),
            config,
            hook_engine,
            idle_fired: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start_background_tasks(self: &Arc<Self>) {
        let engine = self.hook_engine.clone();
        let service = self.clone();
        tokio::spawn(async move {
            let mut rx = engine.subscribe();
            // Use an explicit match so Lagged (consumer fell behind) is treated
            // as a recoverable warning rather than terminating the loop.  Only
            // Closed (sender dropped) should stop hook processing.
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Err(error) = service.process_hook_event(event).await {
                            tracing::warn!(error = %error, "failed to process hook event");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            skipped = n,
                            "hook broadcast channel lagged; {} events dropped, continuing",
                            n
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let engine_5m = self.hook_engine.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SCHEDULE_5M);
            interval.tick().await;
            loop {
                interval.tick().await;
                engine_5m.post(HookEvent {
                    event: "schedule.5m".to_string(),
                    node_id: None,
                    payload: serde_json::json!({}),
                });
            }
        });

        let engine_30m = self.hook_engine.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SCHEDULE_30M);
            interval.tick().await;
            loop {
                interval.tick().await;
                engine_30m.post(HookEvent {
                    event: "schedule.30m".to_string(),
                    node_id: None,
                    payload: serde_json::json!({}),
                });
            }
        });

        // Quiescence idle fallback: fire node.idle for Running local nodes whose
        // harness has no native idle signal (codex) after a configurable window
        // of no PTY output. Claude reports idle natively via hooks and is skipped.
        let quiescence_service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            interval.tick().await;
            loop {
                interval.tick().await;
                quiescence_service.sweep_quiescent_nodes();
            }
        });

        if let (Some(server), Some(topic)) = (&self.config.ntfy_server, &self.config.ntfy_topic) {
            let cfg = NtfyInboundConfig {
                server: server.clone(),
                topic: topic.clone(),
                channel_id: NTFY_DEFAULT_ID.to_string(),
                poll_interval_seconds: self.config.ntfy_poll_interval_seconds.unwrap_or(15),
                token: self.config.ntfy_token.clone(),
            };
            let service = self.clone();
            tokio::spawn(async move {
                ntfy_inbound::run(service, cfg).await;
            });
        }
    }

    pub(crate) fn post_hook_event(&self, event: &str, node_id: Option<Uuid>, payload: JsonValue) {
        self.hook_engine.post(HookEvent {
            event: event.to_string(),
            node_id,
            payload,
        });
    }

    /// Insert an inbound channel message and fire the `channel.inbound` hook event.
    /// Used by the HTTP inbound handler and the ntfy subscriber.
    pub(crate) fn record_channel_inbound(
        &self,
        channel_id: &str,
        sender: &str,
        subject: &str,
        body: &str,
        replies: &[String],
        node_id: Option<Uuid>,
        correlation_token: Option<&str>,
    ) -> Result<()> {
        self.store.insert_channel_message(
            channel_id,
            "in",
            sender,
            subject,
            body,
            replies,
            node_id,
            correlation_token,
        )?;
        self.post_hook_event(
            "channel.inbound",
            node_id,
            serde_json::json!({
                "channel_id": channel_id,
                "sender": sender,
                "subject": subject,
                "body": body,
                "node_id": node_id.map(|id| id.to_string()),
                "correlation_token": correlation_token,
            }),
        );
        Ok(())
    }

    /// Ingest a harness-native signal for a node: map it, store it, post it to
    /// the hook engine, update liveness where meaningful, and record the harness
    /// session id. This is the single producer for the new node.* events.
    pub async fn post_harness_event(
        &self,
        node_id: Uuid,
        request: HarnessEventRequest,
    ) -> Result<HarnessEventResponse> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        let mapped = map_harness_event(&request.source, &request.payload);

        // Record the harness session id (resume key) whenever it is present and
        // has changed. Done even for terminal nodes so a late SessionStart still
        // lands the id.
        if let Some(session_id) = mapped.session_id.as_deref() {
            if node.harness_session_id.as_deref() != Some(session_id) {
                if let Err(e) = self
                    .store
                    .set_node_harness_session_id(node_id, Some(session_id))
                {
                    tracing::warn!(error = %e, node_id = %node_id, "failed to record harness session id");
                }
            }
        }

        // Only live nodes accept behavioural events; terminal nodes are inert.
        if !is_active_liveness(&node.liveness) {
            return Ok(HarnessEventResponse {
                accepted: false,
                event: None,
                session_id: mapped.session_id,
            });
        }

        // Statusline posts drive telemetry + threshold-based ctx_pressure.
        if mapped.telemetry {
            let event =
                self.ingest_statusline(node_id, &request.payload, mapped.session_id.as_deref())?;
            return Ok(HarnessEventResponse {
                accepted: true,
                event,
                session_id: mapped.session_id,
            });
        }

        let Some(kind) = mapped.event else {
            // Recognised source but no mapped event (e.g. auth_success). Accept.
            return Ok(HarnessEventResponse {
                accepted: true,
                event: None,
                session_id: mapped.session_id,
            });
        };

        let mut payload = json!({
            "event": kind,
            "source": request.source,
            "node": { "id": node_id.to_string() },
        });
        if let (Some(target), Some(obj)) = (payload.as_object_mut(), mapped.detail.as_object()) {
            for (key, value) in obj {
                target.insert(key.clone(), value.clone());
            }
        }
        if let Some(session_id) = mapped.session_id.as_deref() {
            if let Some(target) = payload.as_object_mut() {
                target.insert("session_id".to_string(), json!(session_id));
            }
        }

        self.store
            .record_event(node_id, NodeEventKind::HarnessEvent, payload.clone())?;
        self.post_hook_event(kind, Some(node_id), payload);

        // Liveness update, guarded so we don't spam LivenessChanged events for a
        // no-op transition (e.g. repeated tool_call while already Running).
        if let Some(target) = mapped.liveness {
            if node.liveness != target {
                if let Err(e) = self.store.set_node_liveness(node_id, target) {
                    tracing::warn!(error = %e, node_id = %node_id, "failed to update liveness from harness event");
                }
            }
        }

        Ok(HarnessEventResponse {
            accepted: true,
            event: Some(kind.to_string()),
            session_id: mapped.session_id,
        })
    }

    /// Persist a statusline telemetry datapoint and fire `node.ctx_pressure`
    /// when `used_percentage` crosses a configured threshold for the first time
    /// in this session. Returns the mapped event kind if a threshold fired.
    fn ingest_statusline(
        &self,
        node_id: Uuid,
        payload: &JsonValue,
        session_id: Option<&str>,
    ) -> Result<Option<String>> {
        let Some(used) = payload
            .get("context_window")
            .and_then(|c| c.get("used_percentage"))
            .and_then(|v| v.as_f64())
        else {
            return Ok(None);
        };

        // Persist the telemetry datapoint; hydrate_node_telemetry prefers this
        // harness-reported value for the displayed ctx_pct.
        let telemetry_body = json!({
            "event": "node.telemetry",
            "source": "claude_statusline",
            "node": { "id": node_id.to_string() },
            "used_percentage": used,
            "session_id": session_id,
        });
        self.store
            .record_event(node_id, NodeEventKind::HarnessEvent, telemetry_body)?;

        let prior = self.store.harness_event_bodies(node_id).unwrap_or_default();
        let mut thresholds = self.config.autonomy.ctx_pressure_thresholds.clone();
        thresholds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mut fired: Option<String> = None;
        for threshold in thresholds {
            if used < threshold {
                continue;
            }
            let already_fired = prior.iter().any(|body| {
                body.get("event").and_then(|v| v.as_str()) == Some("node.ctx_pressure")
                    && body
                        .get("threshold")
                        .and_then(|v| v.as_f64())
                        .map(|t| (t - threshold).abs() < f64::EPSILON)
                        .unwrap_or(false)
                    && body.get("session_id").and_then(|v| v.as_str()) == session_id
            });
            if already_fired {
                continue;
            }
            let body = json!({
                "event": "node.ctx_pressure",
                "source": "claude_statusline",
                "node": { "id": node_id.to_string() },
                "used_percentage": used,
                "threshold": threshold,
                "session_id": session_id,
            });
            self.store
                .record_event(node_id, NodeEventKind::HarnessEvent, body.clone())?;
            self.post_hook_event("node.ctx_pressure", Some(node_id), body);
            fired = Some("node.ctx_pressure".to_string());
        }
        Ok(fired)
    }

    /// Fire `node.idle` for Running local nodes whose harness has no native idle
    /// signal after the configured quiescence window of no PTY output. Deduped
    /// so it fires once per quiet period and refires only after fresh output.
    fn sweep_quiescent_nodes(&self) {
        let window = self.config.autonomy.idle_quiescence_seconds as i64;
        if window <= 0 {
            return;
        }
        let running = match self.store.list_nodes_by_liveness(NodeLiveness::Running) {
            Ok(nodes) => nodes,
            Err(e) => {
                tracing::warn!(error = %e, "quiescence sweep failed to list running nodes");
                return;
            }
        };
        let now = OffsetDateTime::now_utc().unix_timestamp();
        for node in running {
            if node.substrate != SubstrateKind::Local {
                continue;
            }
            let native_idle = self
                .harnesses
                .get(&node.harness)
                .map(|h| h.native_idle_signal())
                .unwrap_or(false);
            if native_idle {
                continue;
            }
            let last_output = self.store.last_output_chunk_epoch(node.id).ok().flatten();
            let reference = last_output.unwrap_or_else(|| node.created_at.unix_timestamp());
            if now - reference < window {
                continue;
            }
            {
                let mut fired = match self.idle_fired.lock() {
                    Ok(guard) => guard,
                    Err(_) => continue,
                };
                if fired.get(&node.id) == Some(&reference) {
                    continue;
                }
                fired.insert(node.id, reference);
            }
            let body = json!({
                "event": "node.idle",
                "source": "daemon",
                "node": { "id": node.id.to_string() },
                "idle_source": "quiescence",
                "idle_seconds": now - reference,
            });
            if let Err(e) =
                self.store
                    .record_event(node.id, NodeEventKind::HarnessEvent, body.clone())
            {
                tracing::warn!(error = %e, node_id = %node.id, "quiescence sweep failed to record idle");
            }
            self.post_hook_event("node.idle", Some(node.id), body);
        }
    }

    async fn process_hook_event(&self, event: HookEvent) -> Result<()> {
        let rules = self
            .store
            .list_hooks()
            .context("failed to load hooks for event processing")?;
        let mut first_error: Option<anyhow::Error> = None;
        for row in rules {
            if !row.enabled || row.event != event.event {
                continue;
            }
            let row_id = row.id.clone();
            let mut payload = event.payload.clone();
            if let Some(node_id) = event.node_id {
                if let Some(map) = payload.as_object_mut() {
                    let entry = map
                        .entry("node".to_string())
                        .or_insert_with(|| serde_json::json!({}));
                    if let Some(node_obj) = entry.as_object_mut() {
                        node_obj
                            .entry("id".to_string())
                            .or_insert_with(|| JsonValue::String(node_id.to_string()));
                    }
                }
            }
            let rule = match rule_from_row(row) {
                Ok(rule) => rule,
                Err(error) => {
                    let outcome_text = format!("failed to decode hook actions: {error}");
                    if let Err(insert_error) = self.store.insert_hook_firing(
                        &row_id,
                        &event.event,
                        &outcome_text,
                        false,
                        &payload.to_string(),
                    ) {
                        tracing::error!(
                            hook_id = %row_id,
                            error = %insert_error,
                            "failed to persist hook firing for decode failure"
                        );
                        if first_error.is_none() {
                            first_error = Some(insert_error.into());
                        }
                    } else if first_error.is_none() {
                        first_error = Some(anyhow::Error::from(error));
                    }
                    continue;
                }
            };
            if !evaluate_filter(&rule.filter, &payload) {
                continue;
            }
            let outcome = self.execute_hook_actions(&rule, &payload).await;
            let ok = outcome.is_ok();
            let outcome_text = match &outcome {
                Ok(text) => text.clone(),
                Err(error) => error.to_string(),
            };
            if let Err(error) = &outcome {
                if first_error.is_none() {
                    first_error = Some(anyhow::anyhow!("{}", error));
                }
            }
            if let Err(insert_error) = self.store.insert_hook_firing(
                &rule.id,
                &event.event,
                &outcome_text,
                ok,
                &payload.to_string(),
            ) {
                tracing::error!(
                    hook_id = %row_id,
                    error = %insert_error,
                    "failed to persist hook firing"
                );
                if first_error.is_none() {
                    first_error = Some(insert_error.into());
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    async fn execute_hook_actions(&self, rule: &HookRule, payload: &JsonValue) -> Result<String> {
        let mut results: Vec<String> = Vec::new();
        for action in &rule.actions {
            let result = self.execute_hook_action(action, payload).await;
            match result {
                Ok(text) => results.push(text),
                Err(error) => return Err(error),
            }
        }
        Ok(results.join("; "))
    }

    async fn execute_hook_action(
        &self,
        action: &HookAction,
        payload: &JsonValue,
    ) -> Result<String> {
        match action.kind.as_str() {
            "channel" => {
                let channel = require_channel(&self.store, &action.target)?;
                let title = action
                    .args
                    .get("title")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("hook");
                let template = action.template.clone().unwrap_or_else(|| {
                    action
                        .args
                        .get("body")
                        .and_then(JsonValue::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| "{event}".to_string())
                });
                let rendered_title = render_template(title, payload);
                let rendered_body = render_template(&template, payload);
                let node_id = payload_node_id(payload);
                let sent = self
                    .send_via_channel(&channel.id, &rendered_title, &rendered_body, node_id)
                    .await?;
                if !sent {
                    return Err(anyhow!("failed to send through channel '{}'", channel.id));
                }
                Ok(format!("channel:{}", channel.id))
            }
            "spawn" => {
                if !recipe_spawn_is_enabled() {
                    return Err(anyhow!(
                        "hook action kind 'spawn' is unavailable while recipe spawn is disabled"
                    ));
                }
                let target = action.target.clone();
                let recipe_id = target
                    .strip_prefix("recipe:")
                    .ok_or_else(|| anyhow!("spawn target must be 'recipe:<id>'"))?;
                let harness = action
                    .args
                    .get("harness")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("claude_code");
                let substrate = action
                    .args
                    .get("substrate")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("local");
                let request = RecipeSpawnRequest {
                    harness: harness.to_string(),
                    substrate: substrate.to_string(),
                    workspace: action
                        .args
                        .get("workspace")
                        .and_then(JsonValue::as_str)
                        .map(str::to_string),
                    description: None,
                    role_hint: None,
                };
                let response = self.spawn_recipe(recipe_id, request).await?;
                Ok(format!(
                    "spawn:{}:{}",
                    recipe_id,
                    response.node_ids.join(",")
                ))
            }
            "tool" => {
                let outcome = self.dispatch_tool(&action.target, payload).await?;
                Ok(format!("tool:{}:{}", action.target, outcome))
            }
            "pause_node" => {
                let node_id = node_id_from_payload(payload)?;
                self.interrupt_node(node_id).await?;
                Ok(format!("pause_node:{node_id}"))
            }
            "archive" => {
                let node_id = node_id_from_payload(payload)?;
                self.archive_node(node_id).await?;
                Ok(format!("archive:{node_id}"))
            }
            other => Err(anyhow!("unknown hook action kind '{other}'")),
        }
    }

    async fn dispatch_tool(&self, target: &str, _payload: &JsonValue) -> Result<String> {
        if target.starts_with("graph.get") {
            let graph = self.store.graph()?;
            return Ok(format!(
                "nodes={} edges={}",
                graph.nodes.len(),
                graph.relationships.len()
            ));
        }
        if target == "transcript.checkpoint" {
            return Err(anyhow!(
                "tool target 'transcript.checkpoint' is not supported yet"
            ));
        }
        Err(anyhow!("unknown tool target '{target}'"))
    }

    pub async fn send_via_channel(
        &self,
        channel_id: &str,
        title: &str,
        body: &str,
        node_id: Option<Uuid>,
    ) -> Result<bool> {
        let channel = require_channel(&self.store, channel_id)?;
        let mut sent_correlation_token: Option<String> = None;
        let mut body_to_send = body.to_string();
        let effective_node_id = node_id;

        let sent = if !channel.live {
            false
        } else if channel.kind == "ntfy" {
            if let Some(node_id) = node_id {
                let token = next_channel_reply_token();
                let marked_body = ntfy_inbound::append_reply_marker(body, &token);
                if self
                    .send_ntfy(title, &marked_body, None, None, None)
                    .await?
                {
                    let expires_at = OffsetDateTime::now_utc().unix_timestamp()
                        + CHANNEL_REPLY_CORRELATION_TTL_SECONDS;
                    self.store.insert_channel_reply_correlation(
                        &token,
                        &channel.id,
                        node_id,
                        expires_at,
                    )?;
                    body_to_send = marked_body;
                    sent_correlation_token = Some(token);
                    true
                } else {
                    false
                }
            } else {
                self.send_ntfy(title, body, None, None, None).await?
            }
        } else if channel.kind == "webhook" {
            return Err(anyhow!(
                "channel '{}' is inbound-only and cannot be used for outbound delivery",
                channel.id
            ));
        } else {
            return Err(anyhow!(
                "channel '{}' has unsupported outbound kind '{}'",
                channel.id,
                channel.kind
            ));
        };
        let recorded_subject = if sent || channel.kind != "ntfy" {
            title.to_string()
        } else {
            format!("[unsent] {title}")
        };
        self.store.insert_channel_message(
            &channel.id,
            "out",
            "asylum",
            &recorded_subject,
            &body_to_send,
            &[],
            effective_node_id,
            sent_correlation_token.as_deref(),
        )?;
        Ok(sent)
    }
}

fn node_id_from_payload(payload: &JsonValue) -> Result<Uuid> {
    let raw = payload
        .get("node")
        .and_then(|node| node.get("id"))
        .and_then(JsonValue::as_str)
        .or_else(|| payload.get("node_id").and_then(JsonValue::as_str))
        .ok_or_else(|| anyhow!("payload missing node id"))?;
    Ok(Uuid::parse_str(raw)?)
}

fn validate_channel_direction(kind: &str, direction: &str) -> Result<()> {
    match direction {
        "inbound" | "outbound" | "duplex" => {}
        _ => {
            return Err(anyhow!(
                "unsupported channel direction '{direction}' (supported: inbound, outbound, duplex)"
            ));
        }
    }

    if kind == "webhook" && direction != "inbound" {
        return Err(anyhow!(
            "channel kind '{kind}' is inbound-only and cannot use direction '{direction}'"
        ));
    }

    Ok(())
}

fn payload_node_id(payload: &JsonValue) -> Option<Uuid> {
    payload
        .get("node")
        .and_then(|node| node.get("id"))
        .and_then(JsonValue::as_str)
        .or_else(|| payload.get("node_id").and_then(JsonValue::as_str))
        .and_then(|raw| Uuid::parse_str(raw).ok())
}

fn normalize_workspace(workspace: Option<String>) -> Option<String> {
    workspace.and_then(|value| {
        if value.trim().is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

fn next_channel_reply_token() -> String {
    Uuid::new_v4()
        .to_string()
        .replace('-', "")
        .chars()
        .take(CHANNEL_REPLY_TOKEN_LENGTH)
        .collect()
}

fn launch_prompt_for_runtime(
    adapter: &dyn crate::harness::HarnessAdapter,
    node_id: Uuid,
    request: &CreateNodeRequest,
) -> String {
    let context = adapter.launch_context(node_id, request);
    match request
        .description
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        Some(desc) => format!("{}\n\nUser launch packet:\n{}", context.trim_end(), desc),
        None => context,
    }
}

impl CapabilityService {
    pub async fn capabilities(&self) -> CapabilityListResponse {
        self.list_capability_descriptors().await
    }

    pub async fn list_capability_descriptors(&self) -> CapabilityListResponse {
        let has_loon = self.loon_substrate.is_some();
        let capabilities = vec![
            descriptor(
                CapabilityName::ClientConfig,
                "/api/client-config",
                "GET",
                "Read client connection metadata",
                true,
            ),
            descriptor(
                CapabilityName::NodeCreate,
                "/api/nodes",
                "POST",
                "Create a local or configured substrate node",
                true,
            ),
            descriptor(
                CapabilityName::NodeList,
                "/api/nodes",
                "GET",
                "List registered nodes",
                true,
            ),
            descriptor(
                CapabilityName::NodeInspect,
                "/api/nodes/{id}",
                "GET",
                "Inspect one node",
                true,
            ),
            descriptor(
                CapabilityName::NodeObserve,
                "/api/nodes/{id}/observe/ws",
                "WS",
                "Observe node events and output",
                true,
            ),
            descriptor(
                CapabilityName::NodeSendInput,
                "/api/nodes/{id}/input",
                "POST",
                "Send input to a node",
                true,
            ),
            descriptor(
                CapabilityName::NodeInterrupt,
                "/api/nodes/{id}/interrupt",
                "POST",
                "Interrupt a node",
                true,
            ),
            descriptor(
                CapabilityName::NodeStop,
                "/api/nodes/{id}/stop",
                "POST",
                "Stop a node",
                true,
            ),
            descriptor(
                CapabilityName::NodeArchive,
                "/api/nodes/{id}/archive",
                "POST",
                "Archive a node",
                true,
            ),
            descriptor(
                CapabilityName::NodeAttachBrowser,
                "/api/nodes/{id}/attach/browser",
                "POST",
                "Issue an attach-tab URL",
                true,
            ),
            descriptor(
                CapabilityName::NodeAttachNativeTarget,
                "/api/nodes/{id}/attach/native-target",
                "POST",
                "Describe a terminal attach target",
                true,
            ),
            descriptor(
                CapabilityName::RelationshipCreate,
                "/api/relationships",
                "POST",
                "Create an explicit node relationship",
                true,
            ),
            descriptor(
                CapabilityName::RelationshipList,
                "/api/relationships",
                "GET",
                "List explicit node relationships",
                true,
            ),
            descriptor(
                CapabilityName::RelationshipRemove,
                "/api/relationships/{id}",
                "DELETE",
                "Delete a relationship",
                true,
            ),
            descriptor(
                CapabilityName::GraphGet,
                "/api/graph",
                "GET",
                "Read the node graph",
                true,
            ),
            descriptor(
                CapabilityName::HarnessList,
                "/api/harnesses",
                "GET",
                "List configured harness adapters",
                true,
            ),
            descriptor(
                CapabilityName::SubstrateList,
                "/api/substrates",
                "GET",
                "List available substrates",
                true,
            ),
            descriptor(
                CapabilityName::HarnessList,
                "/api/harness-descriptors",
                "GET",
                "List harness adapters with capability descriptors",
                true,
            ),
            descriptor(
                CapabilityName::SubstrateList,
                "/api/substrate-descriptors",
                "GET",
                "List substrates with health and capacity",
                true,
            ),
            descriptor(
                CapabilityName::SubstrateHealth,
                "/api/substrates",
                "GET",
                "Report substrate availability",
                has_loon,
            ),
            descriptor(
                CapabilityName::WorkspaceListRecent,
                "/api/workspaces/recent",
                "GET",
                "List recent node workspaces",
                true,
            ),
            descriptor(
                CapabilityName::ContextCurrentSystemMap,
                "/api/context/system-map",
                "GET",
                "Read the current system map",
                true,
            ),
            descriptor(
                CapabilityName::ContextLaunchPacket,
                "/api/context/launch-packet/{id}",
                "GET",
                "Build a launch packet for a node",
                true,
            ),
            descriptor(
                CapabilityName::NotifySend,
                "/api/notify/send",
                "POST",
                "Send an outbound ntfy notification when configured",
                self.config.ntfy_server.is_some() && self.config.ntfy_topic.is_some(),
            ),
            descriptor(
                CapabilityName::RemoteCommandReceive,
                "/api/remote-commands",
                "POST",
                "Receive and execute a remote command",
                true,
            ),
            descriptor(
                CapabilityName::DecisionRequest,
                "/api/decisions",
                "POST",
                "Create and list operator decisions",
                true,
            ),
            descriptor(
                CapabilityName::DecisionResolve,
                "/api/decisions/{id}/resolve",
                "POST",
                "Resolve an operator decision",
                true,
            ),
            descriptor(
                CapabilityName::TokenIssue,
                "/api/tokens",
                "POST",
                "Issue an owner command token",
                true,
            ),
            descriptor(
                CapabilityName::TokenRevoke,
                "/api/tokens/{id}",
                "DELETE",
                "Revoke an owner command token",
                true,
            ),
            descriptor(
                CapabilityName::AttachUrlIssue,
                "/api/nodes/{id}/attach/browser",
                "POST",
                "Issue a scoped attach URL",
                true,
            ),
            descriptor(
                CapabilityName::ChannelList,
                "/api/channels",
                "GET",
                "List notification channels",
                true,
            ),
            descriptor(
                CapabilityName::ChannelCreate,
                "/api/channels",
                "POST",
                "Create a custom notification channel",
                true,
            ),
            descriptor(
                CapabilityName::ChannelInspect,
                "/api/channels/{id}",
                "GET",
                "Inspect a notification channel",
                true,
            ),
            descriptor(
                CapabilityName::ChannelUpdate,
                "/api/channels/{id}",
                "PATCH",
                "Update a notification channel",
                true,
            ),
            descriptor(
                CapabilityName::ChannelDelete,
                "/api/channels/{id}",
                "DELETE",
                "Delete a custom notification channel",
                true,
            ),
            descriptor(
                CapabilityName::ChannelMessages,
                "/api/channels/{id}/messages",
                "GET",
                "List recent messages on a channel",
                true,
            ),
            descriptor(
                CapabilityName::ChannelTest,
                "/api/channels/{id}/test",
                "POST",
                "Send a test message through a channel",
                true,
            ),
            descriptor(
                CapabilityName::ChannelInbound,
                "/api/channels/{id}/inbound",
                "POST",
                "Record an inbound channel message",
                true,
            ),
            descriptor(
                CapabilityName::HookList,
                "/api/hooks",
                "GET",
                "List hook rules",
                true,
            ),
            descriptor(
                CapabilityName::HookCreate,
                "/api/hooks",
                "POST",
                "Create a hook rule",
                true,
            ),
            descriptor(
                CapabilityName::HookInspect,
                "/api/hooks/{id}",
                "GET",
                "Inspect a hook rule",
                true,
            ),
            descriptor(
                CapabilityName::HookUpdate,
                "/api/hooks/{id}",
                "PATCH",
                "Update a hook rule",
                true,
            ),
            descriptor(
                CapabilityName::HookDelete,
                "/api/hooks/{id}",
                "DELETE",
                "Delete a hook rule",
                true,
            ),
            descriptor(
                CapabilityName::HookFirings,
                "/api/hooks/firings",
                "GET",
                "List recent hook firings",
                true,
            ),
            descriptor(
                CapabilityName::HookEvents,
                "/api/hooks/events",
                "GET",
                "List the hook event catalog",
                true,
            ),
            descriptor(
                CapabilityName::HookTest,
                "/api/hooks/{id}/test",
                "POST",
                "Dry-run a hook rule",
                true,
            ),
            descriptor(
                CapabilityName::RecipeList,
                "/api/recipes",
                "GET",
                "List configured launch recipes (currently none in shipped runtime)",
                true,
            ),
            descriptor(
                CapabilityName::NodeFork,
                "/api/nodes/{id}/fork",
                "POST",
                "Fork a node",
                true,
            ),
            descriptor(
                CapabilityName::NodeSpawnPeer,
                "/api/nodes/{id}/spawn",
                "POST",
                "Spawn a peer node from an existing node",
                true,
            ),
        ];
        CapabilityListResponse { capabilities }
    }

    pub async fn list_nodes(&self) -> Result<NodeListResponse> {
        let nodes = self.store.list_nodes()?;
        Ok(NodeListResponse { nodes })
    }

    pub async fn inspect_node(&self, id: Uuid) -> Result<NodeInspectResponse> {
        let node = self.store.get_node(id)?.context("node not found")?;
        Ok(NodeInspectResponse { node })
    }

    pub async fn node_events(&self, node_id: Uuid) -> Result<NodeEventsResponse> {
        let events = self.store.list_events(node_id)?;
        Ok(NodeEventsResponse { events })
    }

    pub async fn list_node_events(&self, node_id: Uuid) -> Result<NodeEventsResponse> {
        self.node_events(node_id).await
    }

    pub async fn graph(&self) -> Result<GraphGetResponse> {
        let graph = self.store.graph()?;
        Ok(GraphGetResponse { graph })
    }

    pub async fn graph_get(&self) -> Result<GraphGetResponse> {
        self.graph().await
    }

    pub async fn health(&self) -> HealthResponse {
        let database_size_bytes = std::fs::metadata(self.store.path())
            .map(|m| m.len())
            .unwrap_or(0);
        HealthResponse {
            status: "ok".to_string(),
            daemon_version: env!("CARGO_PKG_VERSION").to_string(),
            bind_addr: self.config.bind_addr.clone(),
            base_url: self.config.base_url.clone(),
            socket_path: self.config.socket_path.clone(),
            database_path: self.store.path().to_string(),
            database_size_bytes,
            transcripts_dir: self.config.transcripts_dir.clone(),
        }
    }

    pub async fn list_tokens(&self) -> Result<asylum_types::api::TokenListResponse> {
        let tokens = self.store.list_all_tokens()?;
        Ok(asylum_types::api::TokenListResponse { tokens })
    }

    pub async fn rotate_token(
        &self,
        token_id: Uuid,
    ) -> Result<asylum_types::api::TokenRotateResponse> {
        use crate::auth::issue_owner_token;
        let meta = self
            .store
            .get_token_metadata(token_id)?
            .ok_or_else(|| anyhow!("token not found"))?;
        let (name, created_at, expires_at) = meta;
        // preserve the original ttl (seconds from creation to expiry)
        let ttl_seconds = if expires_at > created_at {
            Some((expires_at - created_at) as u64)
        } else {
            None
        };
        self.store.revoke_token(token_id)?;
        let issued = issue_owner_token(&name, &["owner".to_string()], ttl_seconds)?;
        self.store.insert_token(
            issued.token_id,
            &name,
            &issued.stored_hash,
            &serde_json::to_string(&issued.scope)?,
            issued.expires_at_epoch_secs,
        )?;
        let new_token = asylum_types::api::TokenIssueResponse {
            id: issued.token_id.to_string(),
            raw_token: issued.raw_token,
            scope: issued.scope,
            expires_at_epoch_secs: issued.expires_at_epoch_secs,
        };
        Ok(asylum_types::api::TokenRotateResponse {
            old_id: token_id.to_string(),
            new_token,
        })
    }

    pub async fn client_config(&self) -> ClientConfigResponse {
        ClientConfigResponse {
            base_url: self.config.base_url.clone(),
            capabilities_endpoint: "/api/capabilities".to_string(),
        }
    }

    pub async fn list_harnesses(&self) -> HarnessListResponse {
        let mut items = self
            .harnesses
            .iter()
            .map(|harness| harness.kind().to_string())
            .collect::<Vec<_>>();
        items.sort();
        HarnessListResponse { items }
    }

    pub async fn list_substrates(&self) -> SubstrateListResponse {
        let mut items = vec!["local".to_string()];
        if self.loon_substrate.is_some() {
            items.push("loon".to_string());
        }
        SubstrateListResponse { items }
    }

    pub async fn list_harness_descriptors(&self) -> HarnessDescriptorResponse {
        let mut harnesses = Vec::new();
        for adapter in self.harnesses.iter() {
            let kind = adapter.kind();
            let id = kind.to_string();
            let name = match kind {
                HarnessKind::Codex => "Codex".to_string(),
                HarnessKind::ClaudeCode => "Claude Code".to_string(),
            };
            let snapshot = adapter.capabilities();
            let mut caps = vec!["launch".to_string(), "observe".to_string()];
            if snapshot.browser_attach {
                caps.push("browser_attach".to_string());
            }
            if snapshot.native_attach {
                caps.push("native_attach".to_string());
            }
            if snapshot.send_input {
                caps.push("send_input".to_string());
            }
            if snapshot.interrupt {
                caps.push("interrupt".to_string());
            }
            if snapshot.stop {
                caps.push("stop".to_string());
            }
            if snapshot.resume {
                caps.push("resume".to_string());
            }
            if snapshot.structured_events {
                caps.push("structured_events".to_string());
            }
            if snapshot.transcript_export {
                caps.push("transcript_export".to_string());
            }
            harnesses.push(HarnessDescriptor {
                id,
                name,
                kind: "cli".to_string(),
                available: command_available(adapter.command()),
                command: adapter.command().to_string(),
                caps,
            });
        }
        harnesses.sort_by(|a, b| a.id.cmp(&b.id));
        HarnessDescriptorResponse { harnesses }
    }

    pub async fn list_substrate_descriptors(&self) -> Result<SubstrateDescriptorResponse> {
        let nodes = self.store.list_nodes()?;
        let local_nodes = nodes
            .iter()
            .filter(|n| {
                matches!(n.substrate, SubstrateKind::Local)
                    && matches!(
                        n.liveness,
                        NodeLiveness::Running
                            | NodeLiveness::WaitingForInput
                            | NodeLiveness::Starting
                    )
            })
            .count() as u64;
        let mut substrates = vec![SubstrateDescriptor {
            id: "local".to_string(),
            name: "local".to_string(),
            host: "localhost".to_string(),
            status: "ok".to_string(),
            healthy: true,
            capacity: 0.0,
            nodes: local_nodes,
        }];
        if self.loon_substrate.is_some() {
            let health = self.substrate_health().await;
            let healthy = health.status == "ok";
            let running = health.running_instances.unwrap_or_default();
            let capacity = if running >= 1 {
                f32::min(1.0, running as f32 / 8.0)
            } else {
                0.0
            };
            substrates.push(SubstrateDescriptor {
                id: "loon".to_string(),
                name: "loon".to_string(),
                host: "loon".to_string(),
                status: health.status.clone(),
                healthy,
                capacity,
                nodes: running as u64,
            });
        }
        Ok(SubstrateDescriptorResponse { substrates })
    }

    pub async fn substrate_health(&self) -> SubstrateHealth {
        let status = if let Some(loon) = &self.loon_substrate {
            let health = match loon.health().await {
                Ok(h) => h,
                Err(_) => LoonHealth {
                    status: "unavailable".to_string(),
                    running_instances: None,
                    harness_profiles: None,
                },
            };
            SubstrateHealth {
                status: health.status,
                running_instances: health.running_instances,
                harness_profiles: health.harness_profiles,
            }
        } else {
            SubstrateHealth {
                status: "disabled".to_string(),
                running_instances: None,
                harness_profiles: Some(vec!["local-only".to_string()]),
            }
        };
        status
    }

    pub async fn recent_workspaces(&self) -> Result<Vec<String>> {
        Ok(self
            .store
            .list_recent_workspaces(self.config.workspace_recent_limit)?)
    }

    pub async fn create_node(&self, request: CreateNodeRequest) -> Result<NodeCreateResponse> {
        let mut request = request;
        request.workspace = normalize_workspace(request.workspace);
        let harness = request
            .harness
            .parse::<HarnessKind>()
            .map_err(|err| anyhow!("unknown harness {}: {err}", request.harness))?;
        let substrate = request
            .substrate
            .parse::<SubstrateKind>()
            .map_err(|err| anyhow!("unknown substrate {}: {err}", request.substrate))?;
        let adapter = self
            .harnesses
            .get(&harness)
            .ok_or_else(|| anyhow!("missing harness adapter"))?;
        let capabilities = adapter.capabilities();
        let launch_command = match substrate {
            SubstrateKind::Local => {
                resolve_command(adapter.command()).unwrap_or_else(|| adapter.command().to_string())
            }
            SubstrateKind::Loon => adapter.command().to_string(),
        };

        if matches!(substrate, SubstrateKind::Loon) {
            let loon = self
                .loon_substrate
                .as_ref()
                .ok_or_else(|| anyhow!("unsupported substrate"))?;
            let caps = loom_support_for_harness(loon, &adapter.command(), &harness).await?;
            if !caps.send_input {
                return Err(anyhow!("unsupported_on_substrate"));
            }
        }

        let harness_for_node = harness.clone();
        let node = self.store.insert_node(
            harness_for_node,
            substrate.clone(),
            &request.role_hint,
            request.workspace.as_deref(),
            request.description.as_deref().filter(|v| !v.is_empty()),
            None,
            capabilities.clone(),
            request
                .created_by
                .as_deref()
                .and_then(|id| Uuid::parse_str(id).ok()),
        )?;

        let launch_prompt = launch_prompt_for_runtime(adapter.as_ref(), node.id, &request);
        let mut launch_args = adapter.launch_args().to_vec();
        if matches!(substrate, SubstrateKind::Local) {
            let asylum_binary = current_asylum_binary();
            launch_args.extend(adapter.asylum_control_args(
                &asylum_binary,
                self.config.socket_path.as_deref(),
                node.id,
            ));
        }
        launch_args.extend(request.launch_args.clone());
        // The launch prompt is intentionally NOT appended as a positional argv.
        // Interactive harnesses (claude, codex) pre-fill a positional prompt into
        // the input box but never submit it, so the node sits idle. It is instead
        // delivered over the PTY as a submitted message once the TUI is ready
        // (Local: SubstrateContext::launch_prompt) or as `--prompt` (Loon).
        let env = self.local_launch_env(&node, &harness, &substrate, &capabilities)?;
        let context = SubstrateContext {
            node_id: node.id,
            harness: harness.clone(),
            command: launch_command,
            args: launch_args,
            workspace: request.workspace.clone(),
            env,
            launch_prompt: Some(launch_prompt.clone()),
        };
        match substrate {
            SubstrateKind::Local => {
                // Pre-trust the workspace so harness config dialogs are bypassed before
                // the process even spawns.
                if let Some(ws) = request.workspace.as_deref() {
                    if let Err(e) = adapter.pre_trust_workspace(ws) {
                        let trust_err = anyhow!("pre_trust_workspace failed: {e}");
                        tracing::warn!(
                            error = %e,
                            node_id = %node.id,
                            workspace = ws,
                            "pre_trust_workspace failed — failing launch"
                        );
                        let _ = self.store.set_node_liveness(node.id, NodeLiveness::Failed);
                        let _ = self.store.record_event(
                            node.id,
                            NodeEventKind::HarnessFailure,
                            json!({ "error": trust_err.to_string() }),
                        );
                        return Err(trust_err);
                    }
                }
                if let Err(launch_err) = self.local_substrate.launch(context).await {
                    let _ = self.store.set_node_liveness(node.id, NodeLiveness::Failed);
                    let _ = self.store.record_event(
                        node.id,
                        NodeEventKind::HarnessFailure,
                        json!({ "error": launch_err.to_string() }),
                    );
                    return Err(launch_err);
                }
                self.store
                    .set_node_liveness(node.id, NodeLiveness::Running)?;
            }
            SubstrateKind::Loon => {
                let loon = self
                    .loon_substrate
                    .as_ref()
                    .ok_or_else(|| anyhow!("unsupported substrate"))?;
                let payload = crate::substrate::loon::LoonContext {
                    node_id: node.id,
                    harness: harness.clone(),
                    command: adapter.command().to_string(),
                    prompt: launch_prompt,
                };
                match loon.launch_node(&payload).await {
                    Ok(external_id) => {
                        self.store
                            .set_node_external_id(node.id, Some(external_id))?;
                        self.store
                            .set_node_liveness(node.id, NodeLiveness::Running)?;
                    }
                    Err(launch_err) => {
                        let _ = self.store.set_node_liveness(node.id, NodeLiveness::Failed);
                        let _ = self.store.record_event(
                            node.id,
                            NodeEventKind::HarnessFailure,
                            json!({ "error": launch_err.to_string() }),
                        );
                        return Err(launch_err);
                    }
                }
            }
        }
        self.post_hook_event(
            "graph.spawn",
            Some(node.id),
            json!({
                "node": {"id": node.id.to_string(), "role_hint": node.role_hint, "harness": node.harness.to_string(), "substrate": node.substrate.to_string()},
            }),
        );
        Ok(NodeCreateResponse {
            node_id: node.id.to_string(),
        })
    }

    fn local_launch_env(
        &self,
        node: &NodeRecord,
        harness: &HarnessKind,
        substrate: &SubstrateKind,
        capabilities: &CapabilitySnapshot,
    ) -> Result<Vec<(String, String)>> {
        let mut env = vec![
            ("ASYLUM_NODE_ID".to_string(), node.id.to_string()),
            ("ASYLUM_NODE_ROLE".to_string(), node.role_hint.clone()),
            ("ASYLUM_HARNESS".to_string(), harness.to_string()),
            ("ASYLUM_SUBSTRATE".to_string(), substrate.to_string()),
            ("ASYLUM_BASE_URL".to_string(), self.config.base_url.clone()),
            (
                "ASYLUM_CONTROL_TRANSPORT".to_string(),
                if self.config.socket_path.is_some() {
                    "unix-socket".to_string()
                } else {
                    "http".to_string()
                },
            ),
            (
                "ASYLUM_DECISION_PROTOCOL".to_string(),
                ASYLUM_DECISION_PROTOCOL.to_string(),
            ),
            (
                "ASYLUM_CAPABILITIES_JSON".to_string(),
                serde_json::to_string(capabilities)?,
            ),
            (
                "ASYLUM_GRAPH_SUMMARY".to_string(),
                self.graph_summary()
                    .unwrap_or_else(|_| "graph unavailable".to_string()),
            ),
        ];
        if let Some(workspace) = &node.workspace {
            env.push(("ASYLUM_WORKSPACE".to_string(), workspace.clone()));
        }
        if let Some(socket_path) = &self.config.socket_path {
            env.push(("ASYLUM_SOCKET_PATH".to_string(), socket_path.clone()));
        }
        Ok(env)
    }

    fn graph_summary(&self) -> Result<String> {
        let graph = self.store.graph()?;
        Ok(format!(
            "{} nodes with {} explicit edges",
            graph.nodes.len(),
            graph.relationships.len()
        ))
    }

    pub async fn create_relationship(
        &self,
        request: RelationshipCreateRequest,
    ) -> Result<asylum_types::relationship::RelationshipRecord> {
        let source = Uuid::parse_str(&request.source_node_id)?;
        let target = Uuid::parse_str(&request.target_node_id)?;
        let kind = parse_relationship_kind(&request.kind)?;
        self.store
            .create_relationship(source, target, kind, request.label)
    }

    pub async fn list_relationships(&self) -> Result<RelationshipResponse> {
        let graph = self.store.graph()?;
        Ok(RelationshipResponse {
            relationships: graph.relationships,
        })
    }

    pub async fn delete_relationship(&self, id: Uuid) -> bool {
        self.store.delete_relationship(id).unwrap_or(false)
    }

    pub async fn send_input(&self, node_id: Uuid, payload: SendInputRequest) -> Result<()> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        match node.substrate {
            SubstrateKind::Local => {
                self.local_substrate
                    .send_input(node_id, &payload.text)
                    .await?
            }
            SubstrateKind::Loon => {
                let (loon, external_id) = self.require_loon_target(&node)?;
                loon.send_input(external_id, &payload.text).await?;
            }
        }
        self.store.record_event(
            node_id,
            NodeEventKind::InputSent,
            json!({ "text": payload.text }),
        )?;
        Ok(())
    }

    pub async fn interrupt_node(&self, node_id: Uuid) -> Result<()> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        match node.substrate {
            SubstrateKind::Local => self.local_substrate.interrupt(node_id).await?,
            SubstrateKind::Loon => {
                let (loon, external_id) = self.require_loon_target(&node)?;
                loon.interrupt(external_id).await?;
            }
        }
        // Ctrl-C cancels the current turn; it does NOT terminate the node. The
        // exit sink owns termination truth, so liveness follows the real process
        // signal (or a subsequent harness event) rather than being forced here.
        self.store.record_event(
            node_id,
            NodeEventKind::RemoteCommandReceived,
            json!({"action": "interrupt", "reason": "ctrl_c"}),
        )?;
        Ok(())
    }

    pub async fn stop_node(&self, node_id: Uuid) -> Result<()> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        match node.substrate {
            SubstrateKind::Local => self.local_substrate.stop(node_id).await?,
            SubstrateKind::Loon => {
                let (loon, external_id) = self.require_loon_target(&node)?;
                loon.stop(external_id).await?;
            }
        }
        self.store
            .set_node_liveness(node_id, NodeLiveness::Stopped)?;
        self.post_hook_event(
            "node.exited",
            Some(node_id),
            json!({"node": {"id": node_id.to_string()}, "reason": "stopped"}),
        );
        Ok(())
    }

    pub async fn archive_node(&self, node_id: Uuid) -> Result<()> {
        if let Some(node) = self.store.get_node(node_id)? {
            match node.substrate {
                SubstrateKind::Local => {
                    let _ = self.local_substrate.stop(node_id).await;
                }
                SubstrateKind::Loon => {
                    let (loon, external_id) = self.require_loon_target(&node)?;
                    loon.archive(external_id).await?;
                }
            }
        }
        self.store
            .set_node_liveness(node_id, NodeLiveness::Archived)?;
        self.post_hook_event(
            "node.exited",
            Some(node_id),
            json!({"node": {"id": node_id.to_string()}, "reason": "archived"}),
        );
        Ok(())
    }

    fn require_loon_target<'a>(
        &'a self,
        node: &'a asylum_types::node::NodeRecord,
    ) -> Result<(&'a LoonSubstrate, &'a str)> {
        let loon = self
            .loon_substrate
            .as_deref()
            .ok_or_else(|| anyhow!("loon substrate is not configured"))?;
        let external_id = node
            .external_id
            .as_deref()
            .ok_or_else(|| anyhow!("missing loon external id"))?;
        Ok((loon, external_id))
    }

    pub(crate) async fn require_attachable_node(
        &self,
        node_id: Uuid,
    ) -> Result<asylum_types::node::NodeRecord> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        if !matches!(
            node.liveness,
            NodeLiveness::Running | NodeLiveness::WaitingForInput | NodeLiveness::Starting
        ) {
            return Err(anyhow!(
                "node not attachable in current state: {}",
                node.liveness
            ));
        }

        if node.substrate == SubstrateKind::Local
            && !self.local_substrate.has_runtime(node_id).await
        {
            return Err(anyhow!("local runtime unavailable"));
        }

        if node.substrate == SubstrateKind::Loon {
            self.require_loon_target(&node)?;
        }

        Ok(node)
    }

    pub async fn attach_browser(&self, node_id: Uuid) -> Result<AttachResponse> {
        let node = self.require_attachable_node(node_id).await?;
        let token = self.attach_issuer.issue(node_id, 600)?;
        let fingerprint = &token.raw[..token.raw.len().min(6)];
        self.store.record_event(
            node_id,
            NodeEventKind::AttachIssued,
            json!({ "token_fingerprint": fingerprint }),
        )?;
        let (transport, note) = if node.substrate == SubstrateKind::Loon {
            (
                Some("loon_attach_proxy".to_string()),
                Some("attach tab relays `loon attach`; live PTY-style observe is unavailable for Loon nodes".to_string()),
            )
        } else {
            (Some("local_pty".to_string()), None)
        };
        Ok(AttachResponse {
            url: format!("{}/attach/{}", self.config.base_url, token.raw),
            expires_in_seconds: 600,
            transport,
            note,
        })
    }

    pub async fn attach_native_target(&self, node_id: Uuid) -> Result<NativeAttachResponse> {
        let _node = self.require_attachable_node(node_id).await?;
        let mut environment = std::collections::BTreeMap::new();
        if let Some(socket_path) = &self.config.socket_path {
            environment.insert("ASYLUM_SOCKET_PATH".to_string(), socket_path.clone());
        } else {
            environment.insert("ASYLUM_BASE_URL".to_string(), self.config.base_url.clone());
        }
        Ok(NativeAttachResponse {
            label: "Open in Terminal".to_string(),
            command: "asylum".to_string(),
            args: vec!["attach".to_string(), node_id.to_string()],
            environment,
        })
    }

    pub fn verify_attach_token(&self, token: &str) -> Result<crate::attach::AttachTokenRecord> {
        self.attach_issuer.verify(token)
    }

    pub async fn issue_token(&self, request: TokenRequest) -> Result<TokenIssueResponse> {
        let issued = issue_owner_token(&request.name, &request.scope, request.ttl_seconds)?;
        self.store.insert_token(
            issued.token_id,
            &request.name,
            &issued.stored_hash,
            &serde_json::to_string(&issued.scope)?,
            issued.expires_at_epoch_secs,
        )?;
        Ok(TokenIssueResponse {
            id: issued.token_id.to_string(),
            raw_token: issued.raw_token,
            scope: request.scope,
            expires_at_epoch_secs: issued.expires_at_epoch_secs,
        })
    }

    pub async fn revoke_token(&self, token_id: Uuid) -> Result<bool> {
        self.store.revoke_token(token_id)
    }

    pub fn token_id_for_raw(&self, raw: &str, allow_bootstrap: bool) -> Result<Option<Uuid>> {
        let token_hash = crate::auth::hash_token(raw);
        if let Some((token_id, _, _, _)) = self.store.find_token_by_hash(&token_hash)? {
            return Ok(Some(token_id));
        }

        let is_allowed = match &self.auth_mode {
            AuthMode::Disabled => true,
            AuthMode::OwnerToken { config_token_hash } => {
                config_token_hash.as_deref() == Some(token_hash.as_str())
            }
        };

        if !is_allowed {
            return Err(anyhow!("invalid owner token"));
        }

        if allow_bootstrap && matches!(self.auth_mode, AuthMode::OwnerToken { .. }) {
            return Ok(Some(Uuid::nil()));
        }

        Ok(None)
    }

    pub async fn execute_remote_command(
        &self,
        token_id: Option<Uuid>,
        command: ParsedRemoteCommand,
    ) -> Result<RemoteCommandResponse> {
        let command_id = Uuid::new_v4();
        let command_kind = command.kind;
        let token = command.token;
        let args_json = serde_json::to_string(&command.args)?;
        let args = command.args;
        let request_node_id = command.node_id;
        let args_for_event = args.clone();

        self.store.insert_remote_command(
            command_id,
            command_kind.as_str(),
            &args_json,
            &token,
            request_node_id,
        )?;

        let execution: Result<(Option<Uuid>, serde_json::Value)> = match command_kind {
            RemoteCommandKind::Status => {
                let nodes = self.store.list_nodes()?;
                let running_nodes = nodes
                    .iter()
                    .filter(|node| {
                        matches!(
                            node.liveness,
                            NodeLiveness::Running | NodeLiveness::WaitingForInput
                        )
                    })
                    .count();
                Ok((
                    None,
                    json!({
                        "nodes": nodes.len(),
                        "running_nodes": running_nodes,
                        "message": "status command received",
                    }),
                ))
            }
            RemoteCommandKind::Attach => {
                let node_id = request_node_id.ok_or_else(|| anyhow!("node required"))?;
                match self.attach_browser(node_id).await {
                    Ok(response) => Ok((
                        Some(node_id),
                        json!({
                            "node_id": node_id.to_string(),
                            "url": response.url,
                            "expires_in_seconds": response.expires_in_seconds,
                        }),
                    )),
                    Err(error) => Err(error),
                }
            }
            RemoteCommandKind::SendInput => {
                let node_id = request_node_id.ok_or_else(|| anyhow!("node required"))?;
                let text = args
                    .get("text")
                    .ok_or_else(|| anyhow!("text required"))?
                    .to_string();
                match self.send_input(node_id, SendInputRequest { text }).await {
                    Ok(()) => Ok((
                        Some(node_id),
                        json!({
                            "node_id": node_id.to_string(),
                            "result": "input sent",
                        }),
                    )),
                    Err(error) => Err(error),
                }
            }
            RemoteCommandKind::Start => {
                let harness = args
                    .get("harness")
                    .cloned()
                    .ok_or_else(|| anyhow!("harness required"))?;
                let substrate = args
                    .get("substrate")
                    .cloned()
                    .ok_or_else(|| anyhow!("substrate required"))?;
                let role_hint = args
                    .get("role")
                    .cloned()
                    .unwrap_or_else(|| "worker".to_string());
                let workspace = args.get("workspace").cloned();
                match self
                    .create_node(CreateNodeRequest {
                        harness,
                        substrate,
                        role_hint,
                        workspace,
                        description: None,
                        created_by: None,
                        launch_args: Vec::new(),
                    })
                    .await
                {
                    Ok(response) => Ok((
                        None,
                        json!({
                            "node_id": response.node_id,
                            "result": "node started",
                        }),
                    )),
                    Err(error) => Err(error),
                }
            }
            RemoteCommandKind::Interrupt => {
                let node_id = request_node_id.ok_or_else(|| anyhow!("node required"))?;
                match self.interrupt_node(node_id).await {
                    Ok(()) => Ok((
                        Some(node_id),
                        json!({
                            "node_id": node_id.to_string(),
                            "result": "node interrupted",
                        }),
                    )),
                    Err(error) => Err(error),
                }
            }
            RemoteCommandKind::Stop => {
                let node_id = request_node_id.ok_or_else(|| anyhow!("node required"))?;
                match self.stop_node(node_id).await {
                    Ok(()) => Ok((
                        Some(node_id),
                        json!({
                            "node_id": node_id.to_string(),
                            "result": "node stopped",
                        }),
                    )),
                    Err(error) => Err(error),
                }
            }
            RemoteCommandKind::ApproveDecision => {
                let decision_id = decision_id_from_remote_args(&args)?;
                let decision = self
                    .resolve_decision(
                        decision_id,
                        DecisionResolveRequest {
                            status: "approved".to_string(),
                        },
                    )
                    .await?;
                Ok((
                    None,
                    json!({
                        "decision": decision.id,
                        "status": decision.status,
                    }),
                ))
            }
            RemoteCommandKind::DenyDecision => {
                let decision_id = decision_id_from_remote_args(&args)?;
                let decision = self
                    .resolve_decision(
                        decision_id,
                        DecisionResolveRequest {
                            status: "denied".to_string(),
                        },
                    )
                    .await?;
                Ok((
                    None,
                    json!({
                        "decision": decision.id,
                        "status": decision.status,
                    }),
                ))
            }
        };

        match execution {
            Ok((target_node_id, result)) => {
                if let Some(node_id) = request_node_id {
                    let _ = self.store.record_event(
                        node_id,
                        NodeEventKind::RemoteCommandReceived,
                        json!({
                            "command": command_kind.as_str(),
                            "token_id": token_id.map(|value| value.to_string()),
                            "arguments": args_for_event,
                        }),
                    );
                }

                if let Some(node_id) = target_node_id {
                    let _ = self.store.insert_notification(
                        Some(node_id),
                        "remote_command",
                        "Remote command received",
                        &format!("command={} node_id={}", command_kind.as_str(), node_id,),
                    );
                } else if matches!(command_kind, RemoteCommandKind::Status) {
                    let _ = self.store.insert_notification(
                        None,
                        "remote_command",
                        "Remote command received",
                        "status requested",
                    );
                }

                self.store
                    .update_remote_command_status(command_id, "success", None)?;

                Ok(RemoteCommandResponse {
                    kind: command_kind.as_str().to_string(),
                    status: "success".to_string(),
                    node_id: target_node_id.map(|value| value.to_string()),
                    result,
                })
            }
            Err(error) => {
                let message = error.to_string();
                if let Some(node_id) = request_node_id {
                    let _ = self.store.record_event(
                        node_id,
                        NodeEventKind::RemoteCommandReceived,
                        json!({
                            "command": command_kind.as_str(),
                            "token_id": token_id.map(|value| value.to_string()),
                            "error": &message,
                        }),
                    );
                }
                let _ = self.store.insert_notification(
                    request_node_id,
                    "remote_command",
                    "Remote command failed",
                    &message,
                );
                self.store
                    .update_remote_command_status(command_id, "failed", Some(&message))?;
                Ok(RemoteCommandResponse {
                    kind: command_kind.as_str().to_string(),
                    status: "failed".to_string(),
                    node_id: request_node_id.map(|value| value.to_string()),
                    result: serde_json::json!({ "error": message }),
                })
            }
        }
    }

    pub async fn launch_packet(&self, node_id: Uuid) -> Result<LaunchPacketResponse> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        let graph = self.store.graph()?;
        let caps = [
            ("browser_attach", node.capabilities.browser_attach),
            ("native_attach", node.capabilities.native_attach),
            ("send_input", node.capabilities.send_input),
            ("interrupt", node.capabilities.interrupt),
            ("stop", node.capabilities.stop),
        ];
        let markdown = recipes::launch_packet_markdown(
            &node.id.to_string(),
            &self.config.base_url,
            &node.role_hint,
            &node.harness.to_string(),
            &node.substrate.to_string(),
            &caps,
            &format!(
                "{} nodes with {} explicit edges",
                graph.nodes.len(),
                graph.relationships.len()
            ),
        );
        let artifact_id = self
            .store
            .insert_artifact(
                node_id,
                "launch_packet",
                &format!("{node_id}.md"),
                Some(&markdown),
            )
            .ok()
            .map(|artifact| artifact.to_string());
        Ok(LaunchPacketResponse {
            markdown,
            artifact_id,
        })
    }

    pub async fn list_notifications(&self) -> Result<NotificationsResponse> {
        let notifications = self
            .store
            .list_notifications()?
            .into_iter()
            .map(
                |(id, node_id, kind, title, body, created, read)| Notification {
                    id,
                    kind,
                    title,
                    body,
                    node_id,
                    created_at_epoch_secs: created,
                    read_at_epoch_secs: read,
                },
            )
            .collect();
        Ok(NotificationsResponse { notifications })
    }

    pub async fn mark_notification_read(&self, id: i64) -> Result<()> {
        self.store.mark_notification_read(id)
    }

    pub async fn create_decision(&self, request: DecisionCreateRequest) -> Result<DecisionRecord> {
        if request.text.trim().is_empty() {
            return Err(anyhow!("decision text required"));
        }
        let node_id = request
            .node_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .context("invalid node_id")?;
        if let Some(node_id) = node_id {
            self.store.get_node(node_id)?.context("node not found")?;
        }
        let decision = map_decision(self.store.insert_decision(node_id, request.text.trim())?);
        let _ = self.store.insert_notification(
            node_id,
            "decision",
            "Decision requested",
            &decision.text,
        );
        if let Some(node_id) = node_id {
            let _ = self.store.record_event(
                node_id,
                NodeEventKind::HumanInputRequested,
                json!({
                    "decision": decision.id,
                    "text": decision.text,
                }),
            );
        }
        Ok(decision)
    }

    pub async fn list_decisions(&self) -> Result<DecisionListResponse> {
        Ok(DecisionListResponse {
            decisions: self
                .store
                .list_decisions()?
                .into_iter()
                .map(map_decision)
                .collect(),
        })
    }

    pub async fn get_decision(&self, id: &str) -> Result<DecisionRecord> {
        self.store
            .get_decision(id)?
            .map(map_decision)
            .context("decision not found")
    }

    pub async fn resolve_decision(
        &self,
        id: &str,
        request: DecisionResolveRequest,
    ) -> Result<DecisionRecord> {
        let status = match request.status.as_str() {
            "approved" | "denied" => request.status,
            _ => return Err(anyhow!("decision status must be approved or denied")),
        };
        let before = self.get_decision(id).await?;
        if !self.store.resolve_decision(id, &status)? {
            return Err(anyhow!("decision not found"));
        }
        let after = self.get_decision(id).await?;
        let node_id = before
            .node_id
            .as_deref()
            .and_then(|raw| Uuid::parse_str(raw).ok());
        if let Some(node_id) = node_id {
            if let Ok(Some(node)) = self.store.get_node(node_id) {
                if matches!(node.liveness, NodeLiveness::WaitingForInput) {
                    self.store
                        .set_node_liveness(node_id, NodeLiveness::Running)?;
                }
            }
        }
        let _ = self.store.insert_notification(
            node_id,
            "decision",
            "Decision resolved",
            &format!("{}: {}", after.status, after.text),
        );
        if let Some(node_id) = node_id {
            let _ = self.store.record_event(
                node_id,
                NodeEventKind::RemoteCommandReceived,
                json!({
                    "decision": after.id,
                    "status": after.status,
                }),
            );
        }
        Ok(after)
    }

    pub async fn notify_send(
        &self,
        title: impl AsRef<str>,
        body: impl AsRef<str>,
        server: Option<String>,
        topic: Option<String>,
        token: Option<String>,
    ) -> Result<bool> {
        let sent = self
            .send_ntfy(title.as_ref(), body.as_ref(), server, topic, token)
            .await?;
        if sent {
            let _ = self.store.insert_channel_message(
                NTFY_DEFAULT_ID,
                "out",
                "asylum",
                title.as_ref(),
                body.as_ref(),
                &[],
                None,
                None,
            );
        }
        Ok(sent)
    }

    async fn send_ntfy(
        &self,
        title: &str,
        body: &str,
        server: Option<String>,
        topic: Option<String>,
        token: Option<String>,
    ) -> Result<bool> {
        let configured = asylum_types::config::NtfyConfig {
            server: server.or_else(|| self.config.ntfy_server.clone()),
            topic: topic.or_else(|| self.config.ntfy_topic.clone()),
            token: token.or_else(|| self.config.ntfy_token.clone()),
            poll_interval_seconds: 30,
        };
        if configured.server.is_none() || configured.topic.is_none() {
            return Err(anyhow!("ntfy notification channel is not configured"));
        }
        send_with_optional_config(Some(&configured), title, body).await?;
        Ok(true)
    }

    /// Validate a raw token value (no "Bearer " prefix).
    ///
    /// The static config token (no DB row) is matched by its hash directly.
    /// Every DB-issued token goes through `find_token_by_hash`, which enforces
    /// `revoked = 0 AND expires_at >= now`, so revocation and expiry take effect
    /// immediately on the next request without a daemon restart.
    pub fn validate_owner_token_value(&self, token: &str) -> bool {
        match self.auth_mode {
            AuthMode::Disabled => true,
            AuthMode::OwnerToken {
                ref config_token_hash,
            } => {
                let hash = crate::auth::hash_token(token);
                // Short-circuit only for the static config token; it has no DB row.
                if config_token_hash.as_deref() == Some(hash.as_str()) {
                    return true;
                }
                // All DB-issued tokens must pass the live revocation + expiry check.
                self.store
                    .find_token_by_hash(&hash)
                    .ok()
                    .and_then(|value| value.map(|_| ()))
                    .is_some()
            }
        }
    }

    /// Validate an Authorization header value (accepts "Bearer <token>" or raw token).
    pub fn validate_owner_token(&self, header: Option<&str>) -> bool {
        match self.auth_mode {
            AuthMode::Disabled => true,
            AuthMode::OwnerToken { .. } => {
                let Some(value) = header else {
                    return false;
                };
                let token = value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
                    .unwrap_or(value);
                self.validate_owner_token_value(token)
            }
        }
    }

    pub fn attach_issuer_clone(&self) -> Arc<AttachTokenIssuer> {
        self.attach_issuer.clone()
    }

    pub async fn list_channels(&self) -> Result<ChannelListResponse> {
        let rows = self.store.list_channels()?;
        let channels = rows
            .into_iter()
            .map(|row| descriptor_from_row(&self.store, row))
            .collect::<Result<Vec<_>>>()?;
        Ok(ChannelListResponse { channels })
    }

    pub async fn inspect_channel(&self, id: &str) -> Result<ChannelDescriptor> {
        let row = require_channel(&self.store, id)?;
        descriptor_from_row(&self.store, row)
    }

    pub async fn create_channel(&self, request: ChannelCreateRequest) -> Result<ChannelDescriptor> {
        if !is_implemented_channel_kind(&request.kind) {
            return Err(anyhow!(
                "unsupported channel kind '{}' (supported kinds: ntfy, webhook)",
                request.kind
            ));
        }
        validate_channel_direction(&request.kind, &request.direction)?;

        let id = format!("custom-{}", Uuid::new_v4());
        let label = request.label.unwrap_or_else(|| request.name.clone());
        let row = self.store.upsert_channel(
            &id,
            &request.kind,
            &request.name,
            &label,
            &request.direction,
            if request.live { "live" } else { "configured" },
            &request.detail,
            &request.config.to_string(),
            request.live,
            false,
        )?;
        descriptor_from_row(&self.store, row)
    }

    pub async fn update_channel(
        &self,
        id: &str,
        request: ChannelUpdateRequest,
    ) -> Result<ChannelDescriptor> {
        let existing = require_channel(&self.store, id)?;
        let new_name = request.name.unwrap_or(existing.name);
        let new_label = request.label.unwrap_or(existing.label);
        let new_detail = request.detail.unwrap_or(existing.detail);
        let new_direction = request.direction.unwrap_or(existing.direction);
        validate_channel_direction(&existing.kind, &new_direction)?;
        let new_status = request.status.unwrap_or(existing.status);
        let new_live = request.live.unwrap_or(existing.live);
        let new_config = match request.config {
            Some(value) => value.to_string(),
            None => existing.config_json,
        };
        let row = self.store.upsert_channel(
            id,
            &existing.kind,
            &new_name,
            &new_label,
            &new_direction,
            &new_status,
            &new_detail,
            &new_config,
            new_live,
            existing.builtin,
        )?;
        descriptor_from_row(&self.store, row)
    }

    pub async fn delete_channel(&self, id: &str) -> Result<bool> {
        self.store.delete_channel(id)
    }

    pub async fn channel_messages(
        &self,
        id: &str,
        limit: usize,
    ) -> Result<ChannelMessagesResponse> {
        require_channel(&self.store, id)?;
        let rows = self.store.list_channel_messages(id, limit)?;
        let messages = rows.into_iter().map(message_record_from_row).collect();
        Ok(ChannelMessagesResponse { messages })
    }

    pub async fn channel_test(
        &self,
        id: &str,
        request: ChannelTestRequest,
    ) -> Result<ChannelTestResponse> {
        let sent = self
            .send_via_channel(id, &request.title, &request.body, None)
            .await?;
        Ok(ChannelTestResponse { sent })
    }

    pub async fn channel_inbound(&self, id: &str, request: ChannelInboundRequest) -> Result<()> {
        let channel = require_channel(&self.store, id)?;
        if !channel.live {
            return Err(anyhow!("channel '{id}' is not live"));
        }
        if !matches!(channel.direction.as_str(), "inbound" | "duplex") {
            return Err(anyhow!("channel '{id}' does not accept inbound messages"));
        }
        let node_id = request
            .node_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .context("invalid node_id")?;

        let remote_command_result = self
            .execute_inbound_remote_command_if_present(&request.body)
            .await?;

        if remote_command_result.is_none() {
            if let Some(node_id) = node_id {
                self.send_input(
                    node_id,
                    SendInputRequest {
                        text: request.body.clone(),
                    },
                )
                .await?;
            }
        }

        self.record_channel_inbound(
            id,
            &request.sender,
            &request.subject,
            &request.body,
            &request.replies,
            remote_command_result.flatten().or(node_id),
            request.correlation_token.as_deref(),
        )?;
        Ok(())
    }

    async fn execute_inbound_remote_command_if_present(
        &self,
        body: &str,
    ) -> Result<Option<Option<Uuid>>> {
        if !looks_like_remote_command(body) {
            return Ok(None);
        }
        if !body
            .split_whitespace()
            .skip(1)
            .any(|part| part.starts_with("token="))
        {
            return Ok(None);
        }

        let command = parse_remote_command(body)?;
        if remote_command_requires_node(&command.kind) && command.node_id.is_none() {
            return Err(anyhow!("command requires node"));
        }
        let token_id = self.token_id_for_raw(&command.token, true)?;
        let token_id = token_id.ok_or_else(|| anyhow!("invalid token"))?;
        let response = self.execute_remote_command(Some(token_id), command).await?;
        let node_id = response
            .node_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .context("invalid remote command response node_id")?;
        Ok(Some(node_id))
    }

    pub async fn route_channel_inbound_from_subscriber(
        &self,
        id: &str,
        sender: String,
        subject: String,
        body: String,
        replies: Vec<String>,
        node_id: Option<Uuid>,
        correlation_token: Option<String>,
    ) -> Result<()> {
        let request = ChannelInboundRequest {
            sender,
            subject,
            body,
            replies,
            node_id: node_id.map(|value| value.to_string()),
            correlation_token,
        };
        self.channel_inbound(id, request).await
    }

    pub async fn route_raw_channel_input(&self, node_id: Uuid, body: String) -> Result<()> {
        self.send_input(node_id, SendInputRequest { text: body })
            .await
    }

    pub async fn list_hooks(&self) -> Result<HookListResponse> {
        let rows = self.store.list_hooks()?;
        let hooks = rows
            .into_iter()
            .map(rule_from_row)
            .collect::<Result<Vec<_>>>()?;
        Ok(HookListResponse { hooks })
    }

    pub async fn inspect_hook(&self, id: &str) -> Result<HookRule> {
        let row = self
            .store
            .get_hook(id)?
            .ok_or_else(|| anyhow!("hook '{id}' not found"))?;
        Ok(rule_from_row(row)?)
    }

    pub async fn create_hook(&self, request: HookCreateRequest) -> Result<HookRule> {
        validate_hook_actions(&request.actions)?;
        let id = Uuid::new_v4().to_string();
        let actions_json = serde_json::to_string(&request.actions)?;
        let row = self.store.insert_hook(
            &id,
            &request.name,
            request.enabled,
            &request.event,
            &request.filter,
            &actions_json,
            request.future,
        )?;
        Ok(rule_from_row(row)?)
    }

    pub async fn update_hook(&self, id: &str, request: HookUpdateRequest) -> Result<HookRule> {
        if let Some(ref actions) = request.actions {
            validate_hook_actions(actions)?;
        }
        let actions_json = match request.actions {
            Some(actions) => Some(serde_json::to_string(&actions)?),
            None => None,
        };
        let row = self
            .store
            .update_hook(
                id,
                request.name.as_deref(),
                request.enabled,
                request.event.as_deref(),
                request.filter.as_deref(),
                actions_json.as_deref(),
                request.future,
            )?
            .ok_or_else(|| anyhow!("hook '{id}' not found"))?;
        Ok(rule_from_row(row)?)
    }

    pub async fn delete_hook(&self, id: &str) -> Result<bool> {
        self.store.delete_hook(id)
    }

    pub async fn list_hook_firings(&self, limit: usize) -> Result<HookFiringsResponse> {
        let rows = self.store.list_hook_firings(limit)?;
        let firings = rows.into_iter().map(firing_record_from_row).collect();
        Ok(HookFiringsResponse { firings })
    }

    pub async fn hook_event_catalog(&self) -> HookEventCatalogResponse {
        HookEventCatalogResponse {
            events: event_catalog(),
        }
    }

    pub async fn hook_test(&self, id: &str) -> Result<HookTestResponse> {
        let rule_row = self
            .store
            .get_hook(id)?
            .ok_or_else(|| anyhow!("hook '{id}' not found"))?;
        let rule = rule_from_row(rule_row)?;
        let nodes = self.store.list_nodes()?;
        let target = nodes
            .iter()
            .find(|node| node.is_running_like())
            .map(|node| node.id);
        let mut payload = serde_json::json!({"event": rule.event, "synthetic": true});
        if let Some(node_id) = target {
            payload["node_id"] = JsonValue::String(node_id.to_string());
            if let Some(map) = payload.as_object_mut() {
                map.insert(
                    "node".to_string(),
                    serde_json::json!({"id": node_id.to_string()}),
                );
            }
        }
        let outcome = self.execute_hook_actions(&rule, &payload).await;
        let ok = outcome.is_ok();
        let outcome_text = match &outcome {
            Ok(text) => text.clone(),
            Err(error) => error.to_string(),
        };
        let row = self.store.insert_hook_firing(
            &rule.id,
            "test",
            &outcome_text,
            ok,
            &payload.to_string(),
        )?;
        Ok(HookTestResponse {
            firing: firing_record_from_row(row),
        })
    }

    pub async fn list_recipes(&self) -> RecipeListResponse {
        RecipeListResponse {
            recipes: Vec::new(),
        }
    }

    pub async fn spawn_recipe(
        &self,
        recipe_id: &str,
        _request: RecipeSpawnRequest,
    ) -> Result<RecipeSpawnResponse> {
        Err(anyhow!(
            "recipe-based spawning is disabled until user/config-backed recipes are implemented ({recipe_id})"
        ))
    }

    pub async fn spawn_peer(
        &self,
        source_id: Uuid,
        request: SpawnPeerRequest,
    ) -> Result<SpawnPeerResponse> {
        let source = self
            .store
            .get_node(source_id)?
            .ok_or_else(|| anyhow!("source node not found"))?;
        let harness = request
            .harness
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| source.harness.to_string());
        let substrate = request
            .substrate
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| source.substrate.to_string());
        let role_hint = request
            .role_hint
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "worker".to_string());
        let workspace = request.workspace.or(source.workspace.clone());
        let description = request
            .description
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some(format!("Spawned by Asylum node {source_id}.")));
        let relationship_kind = parse_relationship_kind(
            request
                .relationship_kind
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("spawned_for"),
        )?;
        let relationship_label = request
            .relationship_label
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some("spawn_peer".to_string()));

        let response = self
            .create_node(CreateNodeRequest {
                harness,
                substrate,
                role_hint,
                workspace,
                description,
                created_by: Some(source_id.to_string()),
                launch_args: Vec::new(),
            })
            .await?;
        let new_id = Uuid::parse_str(&response.node_id)?;
        let relationship = self.store.create_relationship(
            source_id,
            new_id,
            relationship_kind,
            relationship_label,
        )?;
        let node = self
            .store
            .get_node(new_id)?
            .ok_or_else(|| anyhow!("spawned node not found"))?;

        Ok(SpawnPeerResponse {
            node_id: response.node_id,
            node,
            relationship,
        })
    }

    pub async fn fork_node(&self, source_id: Uuid, request: ForkNodeRequest) -> Result<NodeRecord> {
        let source = self
            .store
            .get_node(source_id)?
            .ok_or_else(|| anyhow!("source node not found"))?;
        let role_hint = request.role_hint.unwrap_or(source.role_hint.clone());
        let workspace = request.workspace.or(source.workspace.clone());
        let description = request.description.unwrap_or(source.description.clone());
        let response = self
            .create_node(asylum_types::api::CreateNodeRequest {
                harness: source.harness.to_string(),
                substrate: source.substrate.to_string(),
                role_hint,
                workspace,
                description: Some(description),
                created_by: None,
                launch_args: Vec::new(),
            })
            .await?;
        let new_id = Uuid::parse_str(&response.node_id)?;
        self.store.create_relationship(
            source_id,
            new_id,
            RelationshipKind::SpawnedFor,
            Some("fork".to_string()),
        )?;
        let new_node = self
            .store
            .get_node(new_id)?
            .ok_or_else(|| anyhow!("forked node not found"))?;
        Ok(new_node)
    }
}

pub async fn loom_support_for_harness(
    loon: &LoonSubstrate,
    command: &str,
    harness: &HarnessKind,
) -> Result<CapabilitySnapshot> {
    let health = loon.health().await?;
    let requested = if command == "codex" {
        Some(HarnessKind::Codex)
    } else if command == "claude" || command == "claude_code" {
        Some(HarnessKind::ClaudeCode)
    } else {
        None
    };
    let harness = requested.unwrap_or_else(|| harness.clone());
    Ok(capability_flags_from_health(&health, &harness))
}

/// The login-shell PATH, probed once at daemon startup.
/// `None` means the probe was not attempted or produced no output.
static LOGIN_SHELL_PATH: OnceLock<Option<String>> = OnceLock::new();

/// Probe the user's login-shell PATH by running `sh -lc 'echo $PATH'`.
/// Called once at daemon startup; the result is cached for the process lifetime.
pub fn probe_login_shell_path() -> Option<String> {
    let output = std::process::Command::new("sh")
        .args(["-lc", "echo $PATH"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8(output.stdout).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Initialize the login-shell PATH cache.  Call once from daemon startup
/// before any `command_available` invocations.
pub fn init_login_shell_path() {
    LOGIN_SHELL_PATH.get_or_init(probe_login_shell_path);
}

fn command_available(command: &str) -> bool {
    resolve_command(command).is_some()
}

fn resolve_command(command: &str) -> Option<String> {
    let login_path = LOGIN_SHELL_PATH.get().cloned().flatten();
    resolve_command_with_login_path(command, login_path.as_deref())
}

fn current_asylum_binary() -> String {
    env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "asylum".to_string())
}

fn resolve_command_with_login_path(command: &str, login_path: Option<&str>) -> Option<String> {
    if command.trim().is_empty() {
        return None;
    }
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        if is_executable_file(command_path) {
            return Some(command.to_string());
        }
        return None;
    }
    // Search the process PATH first.
    if let Some(paths) = env::var_os("PATH") {
        if let Some(path) = search_paths(command_path, &paths) {
            return Some(path);
        }
    }
    // Fall back to the login-shell PATH cached at startup.  This handles the
    // common case where the daemon is launched by systemd with a sanitized
    // PATH that excludes ~/.local/bin or nvm-managed bin directories.
    if let Some(login_path) = login_path {
        if let Some(path) = env::split_paths(login_path)
            .map(|dir| dir.join(command))
            .find(|candidate| is_executable_file(candidate))
        {
            return Some(path.to_string_lossy().into_owned());
        }
    }
    None
}

fn search_paths(command_path: &Path, paths: &std::ffi::OsStr) -> Option<String> {
    env::split_paths(paths).find_map(|dir| {
        let candidate = dir.join(command_path);
        if is_executable_file(&candidate) {
            Some(candidate.to_string_lossy().into_owned())
        } else {
            None
        }
    })
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let Ok(metadata) = path.metadata() else {
        return false;
    };
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn parse_relationship_kind(raw: &str) -> Result<RelationshipKind> {
    Ok(match raw {
        "supervises" => RelationshipKind::Supervises,
        "spawned_for" => RelationshipKind::SpawnedFor,
        "user_created" => RelationshipKind::UserCreated,
        "platform_responsibility" => RelationshipKind::PlatformResponsibility,
        _ => return Err(anyhow!("unsupported relationship kind")),
    })
}

fn looks_like_remote_command(raw: &str) -> bool {
    matches!(
        raw.split_whitespace().next(),
        Some("status" | "attach" | "send" | "start" | "interrupt" | "stop" | "approve" | "deny")
    )
}

fn recipe_spawn_is_enabled() -> bool {
    false
}

fn validate_hook_actions(actions: &[HookAction]) -> Result<()> {
    if !recipe_spawn_is_enabled() && actions.iter().any(|action| action.kind.as_str() == "spawn") {
        return Err(anyhow!(
            "hook action kind 'spawn' is unavailable while recipe spawn is disabled"
        ));
    }
    Ok(())
}

fn remote_command_requires_node(kind: &RemoteCommandKind) -> bool {
    matches!(
        kind,
        RemoteCommandKind::Attach
            | RemoteCommandKind::SendInput
            | RemoteCommandKind::Interrupt
            | RemoteCommandKind::Stop
    )
}

fn descriptor(
    name: CapabilityName,
    path: &str,
    method: &str,
    description: &str,
    available: bool,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        name,
        path: path.to_string(),
        method: method.to_string(),
        description: description.to_string(),
        available,
    }
}

fn decision_id_from_remote_args(args: &std::collections::HashMap<String, String>) -> Result<&str> {
    args.get("decision")
        .map(String::as_str)
        .ok_or_else(|| anyhow!("decision required"))
}

fn map_decision(
    (id, node_id, text, status, created_at, decided_at): crate::storage::DecisionStorageRecord,
) -> DecisionRecord {
    DecisionRecord {
        id,
        node_id,
        text,
        status,
        created_at_epoch_secs: created_at,
        decided_at_epoch_secs: decided_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::hash_token;
    use crate::channels::WEBHOOK_SUBSTRATE_ID;
    use asylum_types::config::AsylumConfig;
    use rusqlite::Connection;
    use std::ffi::OsString;
    use std::{collections::HashMap, path::Path, sync::Mutex};
    use tokio::time::{sleep, Duration};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set_var(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = env::var_os(key);
            env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn trusted_codex_projects(config_path: &Path) -> Result<Vec<String>> {
        if !config_path.exists() {
            return Ok(Vec::new());
        }

        let raw = std::fs::read_to_string(config_path)?;
        let doc = raw.parse::<toml::Value>()?;
        let projects = doc
            .get("projects")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flat_map(|projects| projects.iter())
            .filter_map(|(path, entry)| {
                let is_trusted =
                    entry.get("trust_level").and_then(toml::Value::as_str) == Some("trusted");
                is_trusted.then(|| path.clone())
            })
            .collect();
        Ok(projects)
    }

    fn assert_no_codex_pretrust_for_workspace_value(
        trusted_projects: &[String],
        workspace_value: &str,
    ) -> Result<()> {
        let cwd = env::current_dir()?.display().to_string();
        assert!(
            !trusted_projects
                .iter()
                .any(|project| project == workspace_value),
            "workspace value should not be pre-trusted: {trusted_projects:?}"
        );
        assert!(
            !trusted_projects.iter().any(|project| project == &cwd),
            "blank/whitespace workspace should not pre-trust the local cwd: {trusted_projects:?}"
        );
        Ok(())
    }

    fn test_app_config() -> AppConfig {
        let core = AsylumConfig::default();
        AppConfig {
            base_url: core.base_url,
            bind_addr: "127.0.0.1:7717".to_string(),
            socket_path: None,
            transcripts_dir: "/tmp/asylum-test/transcripts".to_string(),
            workspace_recent_limit: core.workspace.recent_limit,
            ntfy_server: core.ntfy.server,
            ntfy_topic: core.ntfy.topic,
            ntfy_token: core.ntfy.token,
            ntfy_poll_interval_seconds: Some(core.ntfy.poll_interval_seconds),
            harness: core.harness,
            loon: core.loon,
            autonomy: core.autonomy,
        }
    }

    #[test]
    fn launch_prompt_uses_asylum_context_when_no_user_packet() {
        let registry = crate::harness::HarnessRegistry::default();
        let adapter = registry
            .get(&HarnessKind::Codex)
            .expect("default codex adapter exists");
        let node_id = Uuid::new_v4();
        let request = CreateNodeRequest {
            harness: "codex".to_string(),
            substrate: "local".to_string(),
            role_hint: "worker".to_string(),
            workspace: Some("/tmp".to_string()),
            description: None,
            created_by: None,
            launch_args: Vec::new(),
        };
        let prompt = launch_prompt_for_runtime(adapter.as_ref(), node_id, &request);
        assert!(prompt.contains(&format!("You are node {} with role 'worker'.", node_id)));
        assert!(prompt.contains("Workspace: /tmp"));
        assert!(prompt.contains("System map:"));
        assert!(!prompt.contains("User launch packet"));
    }

    fn local_create_request(description: &str) -> CreateNodeRequest {
        CreateNodeRequest {
            harness: "codex".to_string(),
            substrate: "local".to_string(),
            role_hint: "worker".to_string(),
            workspace: None,
            description: Some(description.to_string()),
            created_by: None,
            launch_args: Vec::new(),
        }
    }

    fn local_create_request_with_workspace(
        description: &str,
        workspace: &str,
    ) -> CreateNodeRequest {
        CreateNodeRequest {
            harness: "codex".to_string(),
            substrate: "local".to_string(),
            role_hint: "worker".to_string(),
            workspace: Some(workspace.to_string()),
            description: Some(description.to_string()),
            created_by: None,
            launch_args: Vec::new(),
        }
    }

    fn loon_create_request(description: &str) -> CreateNodeRequest {
        CreateNodeRequest {
            harness: "codex".to_string(),
            substrate: "loon".to_string(),
            role_hint: "worker".to_string(),
            workspace: None,
            description: Some(description.to_string()),
            created_by: None,
            launch_args: Vec::new(),
        }
    }

    fn loon_create_request_with_workspace(description: &str, workspace: &str) -> CreateNodeRequest {
        CreateNodeRequest {
            harness: "codex".to_string(),
            substrate: "loon".to_string(),
            role_hint: "worker".to_string(),
            workspace: Some(workspace.to_string()),
            description: Some(description.to_string()),
            created_by: None,
            launch_args: Vec::new(),
        }
    }

    fn open_store_with_schema_broken(path: &str, table: &str) -> Result<(), rusqlite::Error> {
        let connection = Connection::open(path)?;
        connection.execute_batch(&format!("DROP TABLE {table};"))?;
        Ok(())
    }

    fn write_executable_script(path: &Path, body: &str) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::write(path, body)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(path)?.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions)?;
        }
        Ok(())
    }

    async fn wait_for_liveness(
        store: &Store,
        node_id: Uuid,
        expected: NodeLiveness,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for _ in 0..50 {
            let node = store.get_node(node_id)?.expect("node should exist");
            if node.liveness == expected {
                return Ok(());
            }
            sleep(Duration::from_millis(20)).await;
        }
        let node = store.get_node(node_id)?.expect("node should exist");
        assert_eq!(node.liveness, expected);
        Ok(())
    }

    #[tokio::test]
    async fn validate_owner_token_value_accepts_configured_token(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let raw = "my-secret-owner-token";
        let service = CapabilityService::new(
            store,
            AuthMode::OwnerToken {
                config_token_hash: Some(hash_token(raw)),
            },
            test_app_config(),
        );
        assert!(service.validate_owner_token_value(raw));
        assert!(!service.validate_owner_token_value("wrong-token"));
        Ok(())
    }

    #[tokio::test]
    async fn token_id_for_raw_uses_active_store_entry() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let issued = issue_owner_token("operator", &["node.list".to_string()], None)?;
        store.insert_token(
            issued.token_id,
            &issued.name,
            &issued.stored_hash,
            &serde_json::to_string(&issued.scope)?,
            issued.expires_at_epoch_secs,
        )?;

        let service = CapabilityService::new(
            store,
            AuthMode::OwnerToken {
                config_token_hash: Some(hash_token("bootstrap")),
            },
            test_app_config(),
        );
        assert_eq!(
            service
                .token_id_for_raw(&issued.raw_token, true)?
                .expect("expected token id"),
            issued.token_id
        );
        assert!(service.token_id_for_raw("not-a-token", true).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn capabilities_hide_recipe_spawn_descriptor() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let capabilities = service.capabilities().await;
        assert!(
            !capabilities
                .capabilities
                .iter()
                .any(|capability| capability.name == CapabilityName::RecipeSpawn),
            "recipe spawn capability descriptor should be hidden while disabled"
        );
        Ok(())
    }

    #[tokio::test]
    async fn create_channel_rejects_unsupported_kinds() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let error = service
            .create_channel(ChannelCreateRequest {
                kind: "email".to_string(),
                name: "email channel".to_string(),
                label: Some("smtp".to_string()),
                direction: "outbound".to_string(),
                detail: "smtp".to_string(),
                config: serde_json::json!({}),
                live: false,
            })
            .await
            .expect_err("unsupported kinds should be rejected");

        assert!(
            error
                .to_string()
                .contains("unsupported channel kind 'email'"),
            "unexpected error: {error}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn create_channel_rejects_webhook_outbound_direction(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let error = service
            .create_channel(ChannelCreateRequest {
                kind: "webhook".to_string(),
                name: "webhook outbound".to_string(),
                label: None,
                direction: "outbound".to_string(),
                detail: "incoming webhook endpoint".to_string(),
                config: serde_json::json!({}),
                live: false,
            })
            .await
            .expect_err("webhook outbound should be rejected");

        assert!(
            error
                .to_string()
                .contains("cannot use direction 'outbound'"),
            "unexpected error: {error}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn update_channel_rejects_webhook_outbound_direction(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let error = service
            .update_channel(
                WEBHOOK_SUBSTRATE_ID,
                ChannelUpdateRequest {
                    name: None,
                    label: None,
                    detail: None,
                    direction: Some("duplex".to_string()),
                    status: None,
                    config: None,
                    live: None,
                },
            )
            .await
            .expect_err("webhook duplex update should be rejected");

        assert!(
            error.to_string().contains("cannot use direction 'duplex'"),
            "unexpected error: {error}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn channel_test_rejects_webhook_outbound_delivery(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let error = service
            .channel_test(
                WEBHOOK_SUBSTRATE_ID,
                ChannelTestRequest {
                    title: "test".to_string(),
                    body: "body".to_string(),
                },
            )
            .await
            .expect_err("webhook should not permit outbound channel tests");
        assert!(
            error.to_string().contains("inbound-only"),
            "unexpected error: {error}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn list_nodes_returns_empty_when_store_is_empty() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let response = service.list_nodes().await?;
        assert!(response.nodes.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn list_nodes_propagates_store_errors() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        open_store_with_schema_broken(&path, "nodes")?;
        let error = service
            .list_nodes()
            .await
            .expect_err("list_nodes should fail when the nodes table is unavailable");
        assert!(error.to_string().contains("no such table: nodes"));
        Ok(())
    }

    #[tokio::test]
    async fn list_substrate_descriptors_propagates_store_errors(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        open_store_with_schema_broken(&path, "nodes")?;
        let error = service.list_substrate_descriptors().await.expect_err(
            "list_substrate_descriptors should fail when the nodes table is unavailable",
        );
        assert!(error.to_string().contains("no such table: nodes"));
        Ok(())
    }

    #[tokio::test]
    async fn list_node_events_returns_empty_when_node_has_no_events(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let response = service.node_events(Uuid::new_v4()).await?;
        assert!(response.events.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn list_node_events_propagates_store_errors() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        open_store_with_schema_broken(&path, "events")?;
        let error = service
            .node_events(Uuid::new_v4())
            .await
            .expect_err("node_events should fail when the events table is unavailable");
        assert!(error.to_string().contains("no such table: events"));
        Ok(())
    }

    #[tokio::test]
    async fn recent_workspaces_propagates_store_errors() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        open_store_with_schema_broken(&path, "nodes")?;
        let error = service
            .recent_workspaces()
            .await
            .expect_err("recent_workspaces should fail when the nodes table is unavailable");
        assert!(error.to_string().contains("no such table: nodes"));
        Ok(())
    }

    #[tokio::test]
    async fn graph_returns_empty_when_store_is_empty() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let response = service.graph().await?;
        assert!(response.graph.nodes.is_empty());
        assert!(response.graph.relationships.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn spawn_peer_creates_real_node_and_explicit_relationship(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = env_lock();
        let workdir = tempfile::tempdir()?;
        let home = workdir.path().join("home");
        std::fs::create_dir_all(&home)?;
        let workspace = workdir.path().join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let workspace_string = workspace.display().to_string();
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let argv_path = workdir.path().join("codex-argv.txt");
        let script_path = workdir.path().join("fake-codex.sh");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            argv_path.display()
        );
        write_executable_script(&script_path, &script)?;

        let _home = EnvVarGuard::set_var("HOME", &home);
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.socket_path = Some("/tmp/asylum-test.sock".to_string());
        config.harness.codex_command = script_path.display().to_string();
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);
        let source = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Local,
            "command-center",
            Some(&workspace_string),
            Some("source node"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;

        let response = service
            .spawn_peer(
                source.id,
                SpawnPeerRequest {
                    description: Some("do useful worker task".to_string()),
                    ..SpawnPeerRequest::default()
                },
            )
            .await?;
        let child_id = Uuid::parse_str(&response.node_id)?;
        wait_for_liveness(&store, child_id, NodeLiveness::Stopped).await?;

        let child = store
            .get_node(child_id)?
            .expect("spawned node should exist");
        assert_eq!(child.harness, HarnessKind::Codex);
        assert_eq!(child.substrate, SubstrateKind::Local);
        assert_eq!(child.role_hint, "worker");
        assert_eq!(child.workspace.as_deref(), Some(workspace_string.as_str()));
        assert_eq!(child.description, "do useful worker task");

        let relationships = store.list_relationships()?;
        let relationship = relationships
            .iter()
            .find(|relationship| {
                relationship.source_node_id == source.id && relationship.target_node_id == child_id
            })
            .expect("spawn_peer should record an explicit edge");
        assert_eq!(relationship.kind, RelationshipKind::SpawnedFor);
        assert_eq!(relationship.label.as_deref(), Some("spawn_peer"));
        assert_eq!(response.relationship.id, relationship.id);

        let argv = std::fs::read_to_string(&argv_path)?;
        assert!(argv.contains("mcp_servers.asylum.args=[\"mcp\"]"));
        assert!(argv.contains(&format!("ASYLUM_NODE_ID=\"{}\"", child_id)));
        assert!(argv.contains("ASYLUM_SOCKET_PATH=\"/tmp/asylum-test.sock\""));
        // The launch prompt (node role/context + Asylum instructions) must NOT
        // ride along as a positional argv any more — it is delivered over the PTY
        // as a submitted message. Its presence in argv would be the old bug-1
        // behavior where the prompt landed in the input box but never submitted.
        assert!(
            !argv.contains("node.spawn_peer"),
            "launch prompt must not be a positional argv: {argv}"
        );
        assert!(!argv.contains("Do not simulate worker nodes inside your own harness session."));
        Ok(())
    }

    #[tokio::test]
    async fn graph_propagates_store_errors() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        open_store_with_schema_broken(&path, "nodes")?;
        let error = service
            .graph()
            .await
            .expect_err("graph should fail when the nodes table is unavailable");
        assert!(error.to_string().contains("no such table: nodes"));
        Ok(())
    }

    #[tokio::test]
    async fn list_substrate_descriptors_hides_unknown_loon_metrics(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.loon.enabled = true;
        let script_path = workdir.path().join("fake-loon-cli.sh");
        config.loon.cli_path = Some(script_path.clone());
        write_executable_script(&script_path, "#!/bin/sh\nexit 0\n")?;

        let service = CapabilityService::new(store, AuthMode::Disabled, config);

        let health = service.substrate_health().await;
        assert_eq!(health.status, "limited");
        assert!(health.running_instances.is_none());
        assert!(health.harness_profiles.is_none());

        let descriptors = service.list_substrate_descriptors().await?;
        let loon = descriptors
            .substrates
            .iter()
            .find(|s| s.id == "loon")
            .expect("loon descriptor should exist when configured");
        assert!(!loon.healthy);
        assert_eq!(loon.status, "limited");
        assert_eq!(loon.capacity, 0.0);
        assert_eq!(loon.nodes, 0);
        Ok(())
    }

    #[tokio::test]
    async fn loon_create_does_not_create_node_when_substrate_support_is_not_confirmed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let mut config = test_app_config();
        config.loon.enabled = true;
        let script_path = workdir.path().join("fake-loon-cli.sh");
        write_executable_script(&script_path, "#!/bin/sh\nprintf 'loon version'\nexit 0\n")?;
        config.loon.cli_path = Some(script_path);

        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let error = service
            .create_node(loon_create_request("unsupported by capability probe"))
            .await
            .expect_err("loon create should refuse unsupported substrate profiles");
        assert!(error.to_string().contains("unsupported_on_substrate"));

        let graph = store.graph()?;
        assert!(
            graph.nodes.is_empty(),
            "no starting node should be created before support check"
        );
        Ok(())
    }

    #[tokio::test]
    async fn failed_loon_spawn_marks_node_failed_and_records_harness_failure(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let mut config = test_app_config();
        config.loon.enabled = true;
        let script_path = workdir.path().join("fake-loon-cli.sh");
        write_executable_script(
            &script_path,
            "#!/bin/sh\nif [ \"$1\" = \"version\" ]; then\nprintf '{\"status\":\"ok\",\"running_instances\":1,\"harness_profiles\":[\"codex\"]}\n'\nexit 0\nfi\nif [ \"$1\" = \"spawn\" ]; then\nprintf 'spawn failed' >&2\nexit 1\nfi\nexit 0\n",
        )?;
        config.loon.cli_path = Some(script_path);

        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let error = service
            .create_node(loon_create_request("spawn failure regression"))
            .await
            .expect_err("spawn failure should return an error and keep an error row");
        assert!(error.to_string().contains("spawn failed"));

        let graph = store.graph()?;
        let node = graph
            .nodes
            .into_iter()
            .find(|node| node.description == "spawn failure regression")
            .expect("failed Loon launch should persist node row");
        assert_eq!(node.liveness, NodeLiveness::Failed);

        let events = store.list_events(node.id)?;
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::LivenessChanged
                && event.body["liveness"] == serde_json::json!("failed")
        }));
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::HarnessFailure
                && event.body["error"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("spawn failed")
        }));
        Ok(())
    }

    #[tokio::test]
    async fn loon_launch_uses_context_plus_user_prompt_in_commandline(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let mut config = test_app_config();
        config.loon.enabled = true;
        let workspace = workdir.path().join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let cli_args_path = workdir.path().join("loon-args.txt");
        let script_path = workdir.path().join("fake-loon-cli.sh");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"version\" ]; then\nprintf '{{\"status\":\"ok\",\"running_instances\":1,\"harness_profiles\":[\"codex\"]}}\\n'\nexit 0\nfi\nif [ \"$1\" = \"spawn\" ]; then\nprintf '%s\\n' \"$@\" > '{}' \nprintf '00000000-0000-0000-0000-000000000000\\n'\nexit 0\nfi\nexit 0\n",
            cli_args_path.display()
        );
        write_executable_script(&script_path, &script)?;
        config.loon.cli_path = Some(script_path);

        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);
        let description = "Build exactly this as your first action";
        let response = service
            .create_node(loon_create_request_with_workspace(
                description,
                &workspace.display().to_string(),
            ))
            .await?;
        let node_id = Uuid::parse_str(&response.node_id)?;

        wait_for_liveness(&store, node_id, NodeLiveness::Running).await?;
        let cli_args = std::fs::read_to_string(&cli_args_path)?;
        assert!(cli_args.contains("spawn"));
        assert!(cli_args.contains("--prompt"));
        assert!(cli_args.contains(&format!("You are node {} with role 'worker'.", node_id)));
        assert!(cli_args.contains(&format!("Workspace: {}", workspace.display())));
        assert!(cli_args.contains("User launch packet:"));
        assert!(cli_args.contains(description));

        Ok(())
    }

    #[tokio::test]
    async fn list_relationships_returns_empty_when_store_is_empty(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let response = service.list_relationships().await?;
        assert!(response.relationships.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn list_relationships_propagates_store_errors() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        open_store_with_schema_broken(&path, "relationships")?;
        let error = service
            .list_relationships()
            .await
            .expect_err("list_relationships should fail when relationship table is unavailable");
        assert!(error.to_string().contains("no such table: relationships"));
        Ok(())
    }

    #[tokio::test]
    async fn execute_remote_command_status_and_send_input_validation(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let status = service
            .execute_remote_command(
                Some(Uuid::nil()),
                ParsedRemoteCommand {
                    kind: RemoteCommandKind::Status,
                    token: "test".to_string(),
                    node_id: None,
                    args: HashMap::new(),
                },
            )
            .await?;
        assert_eq!(status.kind, "status");
        assert_eq!(status.status, "success");
        assert!(status.result["running_nodes"].as_u64().is_some());

        let send_failure = service
            .execute_remote_command(
                Some(Uuid::nil()),
                ParsedRemoteCommand {
                    kind: RemoteCommandKind::SendInput,
                    token: "test".to_string(),
                    node_id: Some(Uuid::new_v4()),
                    args: HashMap::from([("text".to_string(), "hello".to_string())]),
                },
            )
            .await?;
        assert_eq!(send_failure.status, "failed");
        assert_eq!(send_failure.result["error"], "node not found");
        Ok(())
    }

    #[tokio::test]
    async fn remote_decision_resolution_emits_feedback_events(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Local,
            "worker",
            Some("/tmp"),
            Some("decision-node"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;
        let decision = service
            .create_decision(DecisionCreateRequest {
                node_id: Some(node.id.to_string()),
                text: "approve?".to_string(),
            })
            .await?;

        let resolved = service
            .execute_remote_command(
                Some(Uuid::nil()),
                ParsedRemoteCommand {
                    kind: RemoteCommandKind::ApproveDecision,
                    token: "test".to_string(),
                    node_id: None,
                    args: HashMap::from([("decision".to_string(), decision.id.clone())]),
                },
            )
            .await?;

        assert_eq!(resolved.status, "success");
        assert_eq!(resolved.result["status"], "approved");
        assert!(store.list_notifications()?.iter().any(
            |(_, notification_node_id, _, title, _, _, _)| {
                notification_node_id.as_deref() == Some(node.id.to_string().as_str())
                    && title == "Decision resolved"
            }
        ));
        assert!(store.list_events(node.id)?.iter().any(|event| {
            event.kind == NodeEventKind::RemoteCommandReceived
                && event.body["decision"] == decision.id
                && event.body["status"] == "approved"
        }));
        Ok(())
    }

    #[tokio::test]
    async fn list_notifications_propagates_store_errors() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());

        let _ = store.insert_notification(
            None,
            "system",
            "Daemon started",
            "notifications now available",
        )?;

        open_store_with_schema_broken(&path, "notifications")?;
        let error = service
            .list_notifications()
            .await
            .expect_err("list_notifications should fail when notifications table is unavailable");
        assert!(error.to_string().contains("no such table: notifications"));
        Ok(())
    }

    #[test]
    fn command_available_reflects_launchable_executable() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let executable = workdir.path().join("codex-test");
        std::fs::write(&executable, "#!/bin/sh\n")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&executable)?.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&executable, permissions)?;
        }

        assert!(command_available(&executable.display().to_string()));
        assert!(!command_available(""));
        assert!(!command_available(
            &workdir.path().join("missing").display().to_string()
        ));
        Ok(())
    }

    #[test]
    fn command_resolution_uses_login_shell_fallback_path() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let fake_bin = workdir.path().join("login-shell-bin");
        let executable = fake_bin.join("codex-login-shell");
        std::fs::create_dir_all(&fake_bin)?;
        write_executable_script(&executable, "#!/bin/sh\n")?;

        let fallback = fake_bin.display().to_string();
        let resolved = resolve_command_with_login_path("codex-login-shell", Some(&fallback));
        assert_eq!(resolved.as_deref(), Some(executable.to_str().unwrap()));
        assert!(!resolve_command_with_login_path("codex-login-shell", Some("")).is_some());

        Ok(())
    }

    #[tokio::test]
    async fn harness_decision_protocol_ingest_records_pending_decision_event_notification_and_waiting_liveness(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Local,
            "worker",
            Some("/tmp"),
            Some("ingest-node"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;
        let ingester = LocalDecisionIngestion {
            store: store.clone(),
            hook_engine: service.hook_engine.clone(),
        };

        let decision = ingester.ingest_request(
            node.id,
            DecisionProtocolRequest {
                text: "allow this action?".to_string(),
                actions: vec!["approve".to_string(), "deny".to_string()],
                source: Some("permission_prompt".to_string()),
            },
        )?;

        assert_eq!(decision.status, "pending");
        assert_eq!(decision.text, "allow this action?");
        let stored = store.get_decision(&decision.id)?.unwrap();
        assert_eq!(stored.2, "allow this action?");
        let updated_node = store.get_node(node.id)?.expect("node should still exist");
        assert_eq!(updated_node.liveness, NodeLiveness::WaitingForInput);

        assert!(store.list_notifications()?.iter().any(
            |(_, notification_node_id, _, title, body, _, _)| {
                notification_node_id.as_deref() == Some(node.id.to_string().as_str())
                    && title == "Decision requested"
                    && body == "allow this action?"
            }
        ));

        let events = store.list_events(node.id)?;
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::HumanInputRequested
                && event.body["decision"] == decision.id
                && event.body["source"] == serde_json::json!("permission_prompt")
        }));
        Ok(())
    }

    #[tokio::test]
    async fn resolve_decision_restores_running_from_waiting_for_input(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Local,
            "worker",
            Some("/tmp"),
            Some("resolve-node"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;
        store.set_node_liveness(node.id, NodeLiveness::WaitingForInput)?;

        let decision = service
            .create_decision(DecisionCreateRequest {
                node_id: Some(node.id.to_string()),
                text: "approve?".to_string(),
            })
            .await?;
        let decision = service
            .resolve_decision(
                &decision.id,
                DecisionResolveRequest {
                    status: "approved".to_string(),
                },
            )
            .await?;

        assert_eq!(decision.status, "approved");
        let updated_node = store.get_node(node.id)?.expect("node should still exist");
        assert_eq!(updated_node.liveness, NodeLiveness::Running);
        Ok(())
    }

    #[tokio::test]
    async fn manual_create_decision_does_not_mutate_liveness(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Local,
            "worker",
            Some("/tmp"),
            Some("manual-node"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;
        store.set_node_liveness(node.id, NodeLiveness::Running)?;

        let _decision = service
            .create_decision(DecisionCreateRequest {
                node_id: Some(node.id.to_string()),
                text: "manual action".to_string(),
            })
            .await?;
        let updated_node = store.get_node(node.id)?.expect("node should still exist");
        assert_eq!(updated_node.liveness, NodeLiveness::Running);
        Ok(())
    }

    #[tokio::test]
    async fn harness_descriptors_report_missing_commands_unavailable(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.harness.codex_command = workdir.path().join("missing-codex").display().to_string();
        config.harness.claude_command = workdir.path().join("missing-claude").display().to_string();
        let service = CapabilityService::new(store, AuthMode::Disabled, config);

        let response = service.list_harness_descriptors().await;

        assert!(response.harnesses.iter().all(|harness| !harness.available));
        Ok(())
    }

    #[tokio::test]
    async fn local_create_prefers_workspace_pre_trust_for_codex(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = env_lock();
        let test_workdir = tempfile::tempdir()?;
        let workspace = test_workdir.path().join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let script_path = test_workdir.path().join("fake-codex.sh");
        let release_marker = test_workdir.path().join("codex-run-release");
        let script = format!(
            "#!/bin/sh\nwhile [ ! -f '{}' ]; do sleep 0.01; done\n",
            release_marker.display()
        );
        write_executable_script(&script_path, &script)?;

        let (store, node_id) = {
            let _home = EnvVarGuard::set_var("HOME", test_workdir.path());

            let path = test_workdir
                .path()
                .join("asylum.sqlite3")
                .display()
                .to_string();
            let store = Store::open(path)?;
            let mut config = test_app_config();
            config.harness.codex_command = script_path.display().to_string();
            let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);
            let response = service
                .create_node(local_create_request_with_workspace(
                    "pre-trust codex workspace test",
                    &workspace.display().to_string(),
                ))
                .await?;
            let node_id = Uuid::parse_str(&response.node_id)?;

            let codex_config = test_workdir.path().join(".codex").join("config.toml");
            for _ in 0..10 {
                if codex_config.exists() {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
            assert!(codex_config.exists());

            let mut found = false;
            for _ in 0..10 {
                let contents = std::fs::read_to_string(&codex_config)?;
                if contents.contains("trust_level = \"trusted\"")
                    && contents.contains(&workspace.display().to_string())
                {
                    found = true;
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
            assert!(found, "codex workspace was not pre-trusted in config");

            Ok::<(Store, Uuid), Box<dyn std::error::Error>>((store, node_id))
        }?;

        std::fs::write(&release_marker, b"release")?;
        wait_for_liveness(&store, node_id, NodeLiveness::Stopped).await?;
        Ok(())
    }

    #[tokio::test]
    async fn local_create_fails_when_workspace_pre_trust_fails(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = env_lock();
        let test_workdir = tempfile::tempdir()?;
        let workspace = test_workdir.path().join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let script_path = test_workdir.path().join("fake-codex.sh");
        let script = "#!/bin/sh\necho should-not-run\n";
        write_executable_script(&script_path, script)?;

        let bad_home = test_workdir.path().join("bad-home");
        std::fs::write(&bad_home, b"blocked\n")?;

        let path = test_workdir
            .path()
            .join("asylum.sqlite3")
            .display()
            .to_string();
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.harness.codex_command = script_path.display().to_string();
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let error = {
            let _home = EnvVarGuard::set_var("HOME", bad_home);
            service
                .create_node(local_create_request_with_workspace(
                    "pre-trust should fail",
                    &workspace.display().to_string(),
                ))
                .await
                .expect_err("pre-trust failure should reject create")
        };

        assert!(
            error.to_string().contains("pre_trust_workspace failed"),
            "unexpected create error: {error}"
        );

        let nodes = store.list_nodes()?;
        let node = nodes
            .into_iter()
            .find(|node| node.description == "pre-trust should fail")
            .expect("pre-trust failure should still persist a node row");
        assert_eq!(node.liveness, NodeLiveness::Failed);

        let events = store.list_events(node.id)?;
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::HarnessFailure
                && event.body["error"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("pre_trust_workspace failed")
        }));
        Ok(())
    }

    #[tokio::test]
    async fn local_create_normalizes_blank_workspace_to_absent_and_does_not_pre_trust(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = env_lock();
        let test_workdir = tempfile::tempdir()?;
        let home = test_workdir.path().join("home");
        std::fs::create_dir_all(&home)?;
        let script_path = test_workdir.path().join("fake-codex.sh");
        let argv_path = test_workdir.path().join("codex-argv.txt");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            argv_path.display()
        );
        write_executable_script(&script_path, &script)?;

        let response = {
            let _home = EnvVarGuard::set_var("HOME", &home);
            let path = test_workdir
                .path()
                .join("asylum.sqlite3")
                .display()
                .to_string();
            let store = Store::open(path)?;
            let mut config = test_app_config();
            config.harness.codex_command = script_path.display().to_string();
            let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

            let response = service
                .create_node(local_create_request_with_workspace("blank workspace", ""))
                .await?;
            let node_id = Uuid::parse_str(&response.node_id)?;
            wait_for_liveness(&store, node_id, NodeLiveness::Stopped).await?;
            let node = store.get_node(node_id)?.expect("node should exist");
            assert_eq!(node.workspace, None);
            // The launch prompt is delivered over the PTY, not as a positional
            // argv, so the normalized "Workspace: <none>" context must NOT appear
            // in argv. node.workspace == None above already proves normalization.
            let output = std::fs::read_to_string(&argv_path)?;
            assert!(
                !output.contains("Workspace:"),
                "launch prompt must not ride as a positional argv: {output}"
            );
            response
        };

        let codex_config = home.join(".codex").join("config.toml");
        let trusted_projects = trusted_codex_projects(&codex_config)?;
        assert_no_codex_pretrust_for_workspace_value(&trusted_projects, "")?;

        assert!(!response.node_id.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn local_create_normalizes_whitespace_workspace_to_absent_and_does_not_pre_trust(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = env_lock();
        let test_workdir = tempfile::tempdir()?;
        let home = test_workdir.path().join("home");
        std::fs::create_dir_all(&home)?;
        let script_path = test_workdir.path().join("fake-codex.sh");
        let argv_path = test_workdir.path().join("codex-argv.txt");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            argv_path.display()
        );
        write_executable_script(&script_path, &script)?;

        let response = {
            let _home = EnvVarGuard::set_var("HOME", &home);
            let path = test_workdir
                .path()
                .join("asylum.sqlite3")
                .display()
                .to_string();
            let store = Store::open(path)?;
            let mut config = test_app_config();
            config.harness.codex_command = script_path.display().to_string();
            let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

            let response = service
                .create_node(local_create_request_with_workspace(
                    "whitespace workspace",
                    "   ",
                ))
                .await?;
            let node_id = Uuid::parse_str(&response.node_id)?;
            wait_for_liveness(&store, node_id, NodeLiveness::Stopped).await?;
            let node = store.get_node(node_id)?.expect("node should exist");
            assert_eq!(node.workspace, None);
            // The launch prompt is delivered over the PTY, not as a positional
            // argv, so the normalized "Workspace: <none>" context must NOT appear
            // in argv. node.workspace == None above already proves normalization.
            let output = std::fs::read_to_string(&argv_path)?;
            assert!(
                !output.contains("Workspace:"),
                "launch prompt must not ride as a positional argv: {output}"
            );
            response
        };

        let codex_config = home.join(".codex").join("config.toml");
        let trusted_projects = trusted_codex_projects(&codex_config)?;
        assert_no_codex_pretrust_for_workspace_value(&trusted_projects, "   ")?;
        assert!(!response.node_id.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn failed_local_spawn_marks_node_failed_and_records_harness_failure(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.harness.codex_command = workdir.path().join("missing-codex").display().to_string();
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let error = service
            .create_node(local_create_request("spawn failure regression"))
            .await
            .expect_err("missing local harness command should fail launch");
        assert!(error.to_string().contains("spawn local harness process"));

        let graph = store.graph()?;
        let node = graph
            .nodes
            .into_iter()
            .find(|node| node.description == "spawn failure regression")
            .expect("failed launch should leave a durable node row");
        assert_eq!(node.liveness, NodeLiveness::Failed);

        let events = store.list_events(node.id)?;
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::LivenessChanged && event.body["liveness"] == "failed"
        }));
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::HarnessFailure
                && event.body["error"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("spawn local harness process")
        }));
        Ok(())
    }

    #[tokio::test]
    async fn local_launch_does_not_pass_prompt_as_positional_argv_and_persists_early_output(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Bug-1 regression at the capability layer: the launch prompt must not be
        // appended as a positional argv (interactive harnesses never submit it).
        // Actual PTY delivery + submit of the prompt is covered by the
        // substrate-level test `launch_delivers_prompt_over_pty_after_readiness`
        // in substrate/local.rs; here we only assert the prompt is absent from
        // argv and that early TUI output is still captured by the reader.
        let _env = env_lock();
        let workdir = tempfile::tempdir()?;
        let home = workdir.path().join("home");
        std::fs::create_dir_all(&home)?;
        let _home = EnvVarGuard::set_var("HOME", &home);
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let argv_path = workdir.path().join("argv.txt");
        let workspace = workdir.path().join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let script_path = workdir.path().join("fake-codex.sh");
        // Dump argv, emit one TUI frame, then exit cleanly (drives the node to
        // Stopped via the exit sink). No positional prompt should appear here.
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{argv}'\nprintf '\\033[1;1H\\033[0mwelcome'\nsleep 0.2\n",
            argv = argv_path.display(),
        );
        write_executable_script(&script_path, &script)?;

        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.harness.codex_command = script_path.display().to_string();
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let prompt = "Print exactly: hello world from Asylum";
        let response = service
            .create_node(local_create_request_with_workspace(
                prompt,
                &workspace.display().to_string(),
            ))
            .await?;
        let node_id = Uuid::parse_str(&response.node_id)?;

        for _ in 0..100 {
            if argv_path.exists() {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        let argv = std::fs::read_to_string(&argv_path)?;
        let args: Vec<&str> = argv.lines().collect();
        assert_eq!(
            args.first(),
            Some(&"--dangerously-bypass-approvals-and-sandbox")
        );
        let expected_context = format!("You are node {} with role 'worker'.", node_id);
        assert!(
            !argv.contains(&expected_context),
            "launch prompt must not be a positional argv: {argv}"
        );
        assert!(
            !argv.contains("User launch packet:"),
            "user launch packet must not be a positional argv: {argv}"
        );
        assert!(
            !argv.contains(prompt),
            "prompt body must not be a positional argv: {argv}"
        );

        wait_for_liveness(&store, node_id, NodeLiveness::Stopped).await?;
        let events = store.list_events(node_id)?;
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::OutputChunk
                && event.body["text"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("\u{1b}[1;1H\u{1b}[0mwelcome")
        }));
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::LivenessChanged && event.body["liveness"] == "stopped"
        }));
        Ok(())
    }

    #[tokio::test]
    async fn local_create_uses_resolved_login_shell_command_for_launch(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _env_lock = env_lock();
        let workdir = tempfile::tempdir()?;
        let fake_bin = workdir.path().join("login-shell-bin");
        let script = fake_bin.join("codex-login-shell");
        let argv = workdir.path().join("argv.log");
        let release_marker = workdir.path().join("codex-run-release");
        std::fs::create_dir_all(&fake_bin)?;
        let script_body = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$0\" > '{}'\nprintf '%s\\n' \"$@\" >> '{}'\nwhile [ ! -f '{}' ]; do sleep 0.01; done\n",
            argv.display(),
            argv.display(),
            release_marker.display()
        );
        write_executable_script(&script, &script_body)?;

        let runtime_path = "/usr/bin:/bin";
        let login_probe_path = format!("{}:{}", fake_bin.display(), runtime_path);
        let _login_probe_guard = EnvVarGuard::set_var("PATH", &login_probe_path);
        init_login_shell_path();
        let _runtime_path_guard = EnvVarGuard::set_var("PATH", runtime_path);

        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.harness.codex_command = "codex-login-shell".to_string();
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let response = service
            .create_node(local_create_request("launch through fallback command path"))
            .await?;
        let node_id = Uuid::parse_str(&response.node_id)?;
        std::fs::write(&release_marker, b"release")?;

        for _ in 0..80 {
            if argv.exists() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert!(argv.exists());
        let argv_lines = std::fs::read_to_string(&argv)?;
        let first_line = argv_lines
            .lines()
            .next()
            .ok_or_else(|| anyhow!("argv output missing"))?;
        assert_eq!(first_line, script.display().to_string());

        wait_for_liveness(&store, node_id, NodeLiveness::Stopped).await?;
        let events = store.list_events(node_id)?;
        assert!(events.iter().any(|event| {
            event.kind == NodeEventKind::LivenessChanged && event.body["liveness"] == "running"
        }));
        Ok(())
    }

    #[tokio::test]
    async fn notify_send_errors_when_ntfy_is_unconfigured() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let error = service
            .notify_send("title", "body", None, None, None)
            .await
            .expect_err("unconfigured ntfy should not look like a sent=false success");

        assert!(error.to_string().contains("not configured"));
        Ok(())
    }

    #[tokio::test]
    async fn transcript_checkpoint_hook_tool_reports_unsupported(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let hook = service
            .create_hook(HookCreateRequest {
                name: "checkpoint".to_string(),
                enabled: true,
                event: "node.ctx_pressure".to_string(),
                filter: "any".to_string(),
                actions: vec![HookAction {
                    kind: "tool".to_string(),
                    target: "transcript.checkpoint".to_string(),
                    template: None,
                    args: serde_json::json!({}),
                }],
                future: false,
            })
            .await?;

        let response = service.hook_test(&hook.id).await?;

        assert!(!response.firing.ok);
        assert!(response.firing.outcome.contains("not supported yet"));
        Ok(())
    }

    #[tokio::test]
    async fn hook_action_channel_with_webhook_records_failure(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let hook = service
            .create_hook(HookCreateRequest {
                name: "webhook-channel".to_string(),
                enabled: true,
                event: "node.ctx_pressure".to_string(),
                filter: "any".to_string(),
                actions: vec![HookAction {
                    kind: "channel".to_string(),
                    target: WEBHOOK_SUBSTRATE_ID.to_string(),
                    template: Some("{{event}}".to_string()),
                    args: serde_json::json!({"title": "from hook"}),
                }],
                future: false,
            })
            .await?;

        let response = service.hook_test(&hook.id).await?;

        assert!(!response.firing.ok);
        assert!(response.firing.outcome.contains("inbound-only"));
        Ok(())
    }

    #[tokio::test]
    async fn hook_test_propagates_store_errors() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let hook = service
            .create_hook(HookCreateRequest {
                name: "store-error-hook".to_string(),
                enabled: true,
                event: "node.ctx_pressure".to_string(),
                filter: "any".to_string(),
                actions: vec![],
                future: false,
            })
            .await?;

        open_store_with_schema_broken(&path, "nodes")?;
        let error = service
            .hook_test(&hook.id)
            .await
            .expect_err("hook_test should fail when the nodes table is unavailable");
        assert!(error.to_string().contains("no such table: nodes"));
        Ok(())
    }

    #[tokio::test]
    async fn hook_test_fails_for_corrupt_actions_json() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let hook = service
            .create_hook(HookCreateRequest {
                name: "corrupt-json".to_string(),
                enabled: true,
                event: "node.ctx_pressure".to_string(),
                filter: "any".to_string(),
                actions: vec![HookAction {
                    kind: "tool".to_string(),
                    target: "graph.get".to_string(),
                    template: None,
                    args: serde_json::json!({}),
                }],
                future: false,
            })
            .await?;

        let _ = store.update_hook(&hook.id, None, None, None, None, Some("{"), None)?;

        let error = service
            .hook_test(&hook.id)
            .await
            .expect_err("hook_test should fail when stored actions_json is malformed");
        assert!(error.to_string().contains("actions_json"));
        Ok(())
    }

    #[tokio::test]
    async fn hook_event_processor_records_decode_error_when_actions_json_is_corrupt(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let hook = service
            .create_hook(HookCreateRequest {
                name: "corrupt-runtime".to_string(),
                enabled: true,
                event: "node.ctx_pressure".to_string(),
                filter: "any".to_string(),
                actions: vec![HookAction {
                    kind: "tool".to_string(),
                    target: "graph.get".to_string(),
                    template: None,
                    args: serde_json::json!({}),
                }],
                future: false,
            })
            .await?;

        let _ = store.update_hook(&hook.id, None, None, None, None, Some("{"), None)?;

        let process_error = service
            .process_hook_event(HookEvent {
                event: "node.ctx_pressure".to_string(),
                node_id: None,
                payload: serde_json::json!({}),
            })
            .await
            .expect_err("corrupt hook should fail hook-event processing");
        assert!(process_error.to_string().contains("failed to decode hook"));

        let firings = store.list_hook_firings(10)?;
        assert_eq!(firings.len(), 1);
        let firing = firings
            .first()
            .expect("hook processing should persist a failure firing record");
        assert!(!firing.ok);
        assert!(firing.outcome.contains("failed to decode hook actions"));
        Ok(())
    }

    #[tokio::test]
    async fn hook_event_processor_fails_when_firing_persistence_fails(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path.clone())?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let hook = service
            .create_hook(HookCreateRequest {
                name: "drop-table".to_string(),
                enabled: true,
                event: "node.ctx_pressure".to_string(),
                filter: "any".to_string(),
                actions: vec![],
                future: false,
            })
            .await?;
        assert_eq!(hook.actions.len(), 0);

        open_store_with_schema_broken(&path, "hook_firings")?;

        let process_error = service
            .process_hook_event(HookEvent {
                event: "node.ctx_pressure".to_string(),
                node_id: None,
                payload: serde_json::json!({}),
            })
            .await
            .expect_err("missing hook_firings table should surface persistence failure");

        assert!(process_error
            .to_string()
            .contains("no such table: hook_firings"));
        Ok(())
    }

    #[tokio::test]
    async fn hook_actions_reject_spawn_while_disabled() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let create_error = service
            .create_hook(HookCreateRequest {
                name: "spawn-disabled".to_string(),
                enabled: true,
                event: "node.ctx_pressure".to_string(),
                filter: "any".to_string(),
                actions: vec![HookAction {
                    kind: "spawn".to_string(),
                    target: "recipe:quickstart".to_string(),
                    template: None,
                    args: serde_json::json!({}),
                }],
                future: false,
            })
            .await
            .expect_err("hook creation should reject spawn actions");
        assert!(create_error
            .to_string()
            .contains("hook action kind 'spawn'"));

        let hook = service
            .create_hook(HookCreateRequest {
                name: "update-target".to_string(),
                enabled: true,
                event: "node.ctx_pressure".to_string(),
                filter: "any".to_string(),
                actions: vec![],
                future: false,
            })
            .await?;
        let update_error = service
            .update_hook(
                &hook.id,
                HookUpdateRequest {
                    name: Some("update-target".to_string()),
                    enabled: Some(true),
                    event: None,
                    filter: None,
                    actions: Some(vec![HookAction {
                        kind: "spawn".to_string(),
                        target: "recipe:quickstart".to_string(),
                        template: None,
                        args: serde_json::json!({}),
                    }]),
                    future: Some(false),
                },
            )
            .await
            .expect_err("spawn actions should remain rejected on update");
        assert!(update_error
            .to_string()
            .contains("hook action kind 'spawn'"));
        Ok(())
    }

    #[tokio::test]
    async fn routed_channel_inbound_fails_before_recording_when_node_delivery_fails(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Local,
            "worker",
            Some("/tmp"),
            Some("unattached"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;

        let error = service
            .channel_inbound(
                "webhook-substrate",
                ChannelInboundRequest {
                    sender: "smoke".to_string(),
                    subject: "route".to_string(),
                    body: "must not persist".to_string(),
                    replies: vec![],
                    node_id: Some(node.id.to_string()),
                    correlation_token: Some("corr".to_string()),
                },
            )
            .await
            .expect_err("routing to an unattached local runtime should fail");

        assert!(
            error.to_string().contains("node not running"),
            "unexpected error: {error}"
        );
        assert!(store
            .list_channel_messages("webhook-substrate", 10)?
            .into_iter()
            .all(|message| message.body != "must not persist"));
        Ok(())
    }

    #[tokio::test]
    async fn loon_controls_fail_without_configured_target_before_mutating_state(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());

        let send_node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Loon,
            "worker",
            Some("/tmp"),
            Some("send"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;
        let send_error = service
            .send_input(
                send_node.id,
                SendInputRequest {
                    text: "hello".to_string(),
                },
            )
            .await
            .expect_err("send should fail when Loon is not configured");
        assert!(send_error.to_string().contains("not configured"));
        assert!(store
            .list_events(send_node.id)?
            .iter()
            .all(|event| event.kind != NodeEventKind::InputSent));

        for operation in ["interrupt", "stop", "archive"] {
            let node = store.insert_node(
                HarnessKind::Codex,
                SubstrateKind::Loon,
                "worker",
                Some("/tmp"),
                Some(operation),
                None,
                CapabilitySnapshot::default(),
                None,
            )?;
            let error = match operation {
                "interrupt" => service.interrupt_node(node.id).await,
                "stop" => service.stop_node(node.id).await,
                "archive" => service.archive_node(node.id).await,
                _ => unreachable!(),
            }
            .expect_err("operation should fail when Loon is not configured");
            assert!(error.to_string().contains("not configured"));
            let stored = store.get_node(node.id)?.expect("node should remain stored");
            assert_eq!(stored.liveness, NodeLiveness::Starting);
        }
        Ok(())
    }

    #[tokio::test]
    async fn attach_rejects_stopped_failed_and_archived_nodes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());

        for liveness in [
            NodeLiveness::Stopped,
            NodeLiveness::Failed,
            NodeLiveness::Archived,
        ] {
            let node = store.insert_node(
                HarnessKind::Codex,
                SubstrateKind::Local,
                "worker",
                Some("/tmp"),
                Some("attach state rejection"),
                None,
                CapabilitySnapshot::default(),
                None,
            )?;
            store.set_node_liveness(node.id, liveness)?;

            let browser_error = service
                .attach_browser(node.id)
                .await
                .expect_err("browser attach should fail for non-attachable node");
            assert!(
                browser_error.to_string().contains("node not attachable"),
                "unexpected browser attach error: {browser_error}"
            );
            let native_error = service
                .attach_native_target(node.id)
                .await
                .expect_err("native attach should fail for non-attachable node");
            assert!(
                native_error.to_string().contains("node not attachable"),
                "unexpected native attach error: {native_error}"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn attach_rejects_local_runtime_missing_nodes() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());

        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Local,
            "worker",
            Some("/tmp"),
            Some("stale local runtime"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;
        store.set_node_liveness(node.id, NodeLiveness::Running)?;

        let browser_error = service
            .attach_browser(node.id)
            .await
            .expect_err("browser attach should fail when local runtime is unavailable");
        assert!(browser_error
            .to_string()
            .contains("local runtime unavailable"));

        let native_error = service
            .attach_native_target(node.id)
            .await
            .expect_err("native attach should fail when local runtime is unavailable");
        assert!(native_error
            .to_string()
            .contains("local runtime unavailable"));

        Ok(())
    }

    #[tokio::test]
    async fn attach_is_allowed_for_running_local_node_with_runtime(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let script_path = workdir.path().join("fake-codex.sh");
        write_executable_script(
            &script_path,
            "#!/bin/sh\nwhile true; do\n  sleep 0.1\ndone\n",
        )?;
        let mut config = test_app_config();
        config.harness.codex_command = script_path.display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let response = service
            .create_node(local_create_request("attachable local"))
            .await?;
        let node_id = Uuid::parse_str(&response.node_id)?;
        wait_for_liveness(&store, node_id, NodeLiveness::Running).await?;

        let browser = service.attach_browser(node_id).await?;
        assert_eq!(browser.transport.as_deref(), Some("local_pty"));
        assert!(browser.url.contains(&node_id.to_string()));

        let native = service.attach_native_target(node_id).await?;
        assert_eq!(native.command, "asylum");
        assert_eq!(native.args.first().map(String::as_str), Some("attach"));
        assert_eq!(
            native.args.get(1).map(String::as_str),
            Some(node_id.to_string().as_str())
        );

        service.stop_node(node_id).await?;
        wait_for_liveness(&store, node_id, NodeLiveness::Stopped).await?;
        Ok(())
    }

    #[tokio::test]
    async fn attach_to_loon_rejects_without_configured_substrate(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());

        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Loon,
            "worker",
            Some("/tmp"),
            Some("loon"),
            Some("loon-instance-1"),
            CapabilitySnapshot::default(),
            None,
        )?;

        let error = service
            .attach_browser(node.id)
            .await
            .expect_err("browser attach should fail without loon substrate");
        assert!(error
            .to_string()
            .contains("loon substrate is not configured"));
        Ok(())
    }

    #[tokio::test]
    async fn attach_to_loon_rejects_without_external_id() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.loon.enabled = true;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Loon,
            "worker",
            Some("/tmp"),
            Some("loon without external id"),
            None,
            CapabilitySnapshot::default(),
            None,
        )?;

        let error = service
            .attach_browser(node.id)
            .await
            .expect_err("browser attach should fail without loon external id");
        assert!(error.to_string().contains("missing loon external id"));
        Ok(())
    }

    #[tokio::test]
    async fn loon_browser_attach_response_discloses_transport(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.loon.enabled = true;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let node = store.insert_node(
            HarnessKind::Codex,
            SubstrateKind::Loon,
            "worker",
            Some("/tmp"),
            Some("loon"),
            Some("loon-instance-1"),
            CapabilitySnapshot::default(),
            None,
        )?;

        let response = service.attach_browser(node.id).await?;

        assert_eq!(response.transport.as_deref(), Some("loon_attach_proxy"));
        assert!(response
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("loon attach"));
        Ok(())
    }

    /// H1: a DB-issued token that is revoked must no longer authenticate, even
    /// though it was once valid (i.e. the in-memory snapshot, if it existed,
    /// would still match).  After `store.revoke_token`, the next call to
    /// `validate_owner_token_value` must return false because we always consult
    /// `find_token_by_hash` for DB-issued tokens.
    #[tokio::test]
    async fn revoked_db_token_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;

        // Issue and persist a DB token.
        let issued = issue_owner_token("revoke-test", &["*".to_string()], Some(3600))?;
        store.insert_token(
            issued.token_id,
            &issued.name,
            &issued.stored_hash,
            &serde_json::to_string(&issued.scope)?,
            issued.expires_at_epoch_secs,
        )?;

        // No config-static token; auth relies entirely on the DB.
        let service = CapabilityService::new(
            store.clone(),
            AuthMode::OwnerToken {
                config_token_hash: None,
            },
            test_app_config(),
        );

        // Token is valid before revocation.
        assert!(
            service.validate_owner_token_value(&issued.raw_token),
            "token should authenticate before revocation"
        );

        // Revoke it.
        store.revoke_token(issued.token_id)?;

        // Token must now be rejected — revocation is enforced on every request.
        assert!(
            !service.validate_owner_token_value(&issued.raw_token),
            "revoked token must not authenticate"
        );
        Ok(())
    }

    /// H2: the hook consumer must survive a broadcast::Lagged error and continue
    /// processing subsequent events.
    ///
    /// Proof-of-concept: subscribe first (so the receiver is live), then
    /// without draining, flood the 256-slot channel with 300 messages.  The
    /// next recv() will return Err(Lagged(n)) because the slow consumer fell
    /// behind.  Assert Lagged is returned, then assert that the receiver
    /// recovers and can read subsequent messages — i.e. the channel is NOT
    /// closed and a simple `continue` in the production loop is the right fix.
    #[tokio::test]
    async fn hook_consumer_continues_after_broadcast_lag() {
        use tokio::sync::broadcast::error::RecvError;

        let engine = crate::hooks::HookEngine::new();

        // Subscribe first so this receiver is registered and can fall behind.
        let mut rx = engine.subscribe();

        // Flood the 256-slot channel with 300 messages without draining rx.
        // This forces the internal ring buffer to wrap, dropping the oldest
        // 44 messages from rx's perspective.
        for i in 0..300u32 {
            engine.post(HookEvent {
                event: format!("schedule.test.{i}"),
                node_id: None,
                payload: serde_json::json!({}),
            });
        }

        // First recv must be Lagged because we overflowed without draining.
        let first = rx.recv().await;
        assert!(
            matches!(first, Err(RecvError::Lagged(_))),
            "expected Lagged on first recv after overflow, got {first:?}"
        );

        // After a Lagged the receiver is repositioned to the oldest retained
        // message.  Subsequent recvs must succeed — the channel is still open.
        let second = rx.recv().await;
        assert!(
            second.is_ok(),
            "consumer should recover after Lagged and read the next retained message, got {second:?}"
        );
    }

    #[tokio::test]
    async fn health_response_includes_daemon_version_and_paths(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let config = test_app_config();
        let service = CapabilityService::new(store, AuthMode::Disabled, config);
        let response = service.health().await;
        assert_eq!(response.daemon_version, env!("CARGO_PKG_VERSION"));
        assert!(!response.database_path.is_empty());
        assert_eq!(response.bind_addr, "127.0.0.1:7717");
        assert!(!response.transcripts_dir.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn list_tokens_returns_metadata_only() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(
            store,
            AuthMode::OwnerToken {
                config_token_hash: Some(hash_token("bootstrap")),
            },
            test_app_config(),
        );

        use asylum_types::security::TokenRequest;
        service
            .issue_token(TokenRequest {
                name: "token-a".to_string(),
                scope: vec!["owner".to_string()],
                ttl_seconds: Some(3600),
            })
            .await?;
        service
            .issue_token(TokenRequest {
                name: "token-b".to_string(),
                scope: vec!["owner".to_string()],
                ttl_seconds: Some(3600),
            })
            .await?;

        let list = service.list_tokens().await?;
        assert_eq!(list.tokens.len(), 2);

        // must never include raw_token or hash
        let token_value = serde_json::to_value(&list.tokens[0]).unwrap();
        let obj = token_value.as_object().unwrap();
        assert!(
            !obj.contains_key("raw_token"),
            "raw_token must not be in token list"
        );
        assert!(!obj.contains_key("hash"), "hash must not be in token list");
        assert!(!obj.contains_key("raw"), "raw must not be in token list");
        Ok(())
    }

    #[tokio::test]
    async fn rotate_token_revokes_old_and_issues_new() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(
            store.clone(),
            AuthMode::OwnerToken {
                config_token_hash: Some(hash_token("bootstrap")),
            },
            test_app_config(),
        );

        use asylum_types::security::TokenRequest;
        let issued = service
            .issue_token(TokenRequest {
                name: "operator".to_string(),
                scope: vec!["owner".to_string()],
                ttl_seconds: Some(3600),
            })
            .await?;

        let old_id = Uuid::parse_str(&issued.id)?;
        assert!(service.validate_owner_token_value(&issued.raw_token));

        let rotated = service.rotate_token(old_id).await?;
        assert_eq!(rotated.old_id, issued.id);
        // old token must now be revoked
        assert!(!service.validate_owner_token_value(&issued.raw_token));
        // new token must be valid
        assert!(service.validate_owner_token_value(&rotated.new_token.raw_token));
        Ok(())
    }

    #[tokio::test]
    async fn recipes_are_disabled_and_not_preseeded() -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let response = service.list_recipes().await;
        assert!(response.recipes.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn spawn_recipe_returns_error_until_configured() -> Result<(), Box<dyn std::error::Error>>
    {
        let workdir = tempfile::tempdir()?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path)?;
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());

        let error = service
            .spawn_recipe(
                "start-command-center",
                RecipeSpawnRequest {
                    harness: "codex".to_string(),
                    substrate: "local".to_string(),
                    workspace: None,
                    description: None,
                    role_hint: Some("command-center".to_string()),
                },
            )
            .await
            .expect_err("spawning recipes should be disabled");
        assert!(error
            .to_string()
            .contains("recipe-based spawning is disabled"));
        Ok(())
    }

    // ---- W1: harness-event ingestion ----------------------------------------

    fn open_test_store() -> (tempfile::TempDir, Store) {
        let workdir = tempfile::tempdir().expect("tempdir");
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let store = Store::open(path).expect("open store");
        (workdir, store)
    }

    fn insert_active_node(store: &Store, harness: HarnessKind) -> NodeRecord {
        store
            .insert_node(
                harness,
                SubstrateKind::Local,
                "worker",
                None,
                Some("harness-event target"),
                None,
                CapabilitySnapshot::default(),
                None,
            )
            .expect("insert node")
    }

    #[test]
    fn map_harness_event_maps_claude_stop_to_turn_complete() {
        let payload = json!({
            "hook_event_name": "Stop",
            "session_id": "sess-stop",
            "transcript_path": "/tmp/t.jsonl",
            "cwd": "/work"
        });
        let mapped = map_harness_event("claude_hook", &payload);
        assert_eq!(mapped.event, Some("node.turn_complete"));
        assert_eq!(mapped.liveness, Some(NodeLiveness::Running));
        assert_eq!(mapped.session_id.as_deref(), Some("sess-stop"));
        assert!(!mapped.telemetry);
    }

    #[test]
    fn map_harness_event_maps_claude_notification_permission_to_awaiting_input() {
        let payload = json!({
            "hook_event_name": "Notification",
            "type": "permission_prompt",
            "message": "Claude needs your permission to use Bash",
            "session_id": "sess-perm"
        });
        let mapped = map_harness_event("claude_hook", &payload);
        assert_eq!(mapped.event, Some("node.awaiting_input"));
        assert_eq!(mapped.liveness, Some(NodeLiveness::WaitingForInput));
        assert_eq!(mapped.detail["type"], json!("permission_prompt"));
    }

    #[test]
    fn map_harness_event_maps_claude_notification_idle() {
        let payload = json!({
            "hook_event_name": "Notification",
            "type": "idle_prompt",
            "message": "Claude is waiting for your input",
            "session_id": "sess-idle"
        });
        let mapped = map_harness_event("claude_hook", &payload);
        assert_eq!(mapped.event, Some("node.idle"));
        assert_eq!(mapped.liveness, Some(NodeLiveness::Running));
    }

    #[test]
    fn map_harness_event_maps_claude_session_start() {
        let payload = json!({
            "hook_event_name": "SessionStart",
            "source": "startup",
            "model": "claude-opus",
            "session_id": "sess-start"
        });
        let mapped = map_harness_event("claude_hook", &payload);
        assert_eq!(mapped.event, Some("node.session_started"));
        assert_eq!(mapped.liveness, Some(NodeLiveness::Running));
        assert_eq!(mapped.detail["source"], json!("startup"));
        assert_eq!(mapped.session_id.as_deref(), Some("sess-start"));
    }

    #[test]
    fn map_harness_event_maps_codex_agent_turn_complete() {
        let payload = json!({
            "type": "agent-turn-complete",
            "thread-id": "6a1f-thread",
            "turn-id": "turn-9",
            "cwd": "/work",
            "input-messages": ["do the thing"],
            "last-assistant-message": "did the thing"
        });
        let mapped = map_harness_event("codex_notify", &payload);
        assert_eq!(mapped.event, Some("node.turn_complete"));
        assert_eq!(mapped.liveness, Some(NodeLiveness::Running));
        assert_eq!(mapped.session_id.as_deref(), Some("6a1f-thread"));
        assert_eq!(
            mapped.detail["last_assistant_message"],
            json!("did the thing")
        );
    }

    #[test]
    fn map_harness_event_marks_statusline_as_telemetry() {
        let payload = json!({
            "session_id": "sess-line",
            "context_window": { "used_percentage": 42.0, "remaining_percentage": 58.0 }
        });
        let mapped = map_harness_event("claude_statusline", &payload);
        assert!(mapped.telemetry);
        assert_eq!(mapped.event, None);
        assert_eq!(mapped.session_id.as_deref(), Some("sess-line"));
    }

    #[tokio::test]
    async fn post_harness_event_records_session_and_transitions_liveness(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (_workdir, store) = open_test_store();
        let node = insert_active_node(&store, HarnessKind::ClaudeCode);
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());

        // SessionStart records the harness session id and keeps the node live.
        let response = service
            .post_harness_event(
                node.id,
                HarnessEventRequest {
                    source: "claude_hook".to_string(),
                    payload: json!({
                        "hook_event_name": "SessionStart",
                        "source": "startup",
                        "session_id": "sess-abc"
                    }),
                },
            )
            .await?;
        assert!(response.accepted);
        assert_eq!(response.event.as_deref(), Some("node.session_started"));
        let refreshed = store.get_node(node.id)?.expect("node exists");
        assert_eq!(refreshed.harness_session_id.as_deref(), Some("sess-abc"));

        // Awaiting-input moves liveness to WaitingForInput.
        service
            .post_harness_event(
                node.id,
                HarnessEventRequest {
                    source: "claude_hook".to_string(),
                    payload: json!({
                        "hook_event_name": "Notification",
                        "type": "agent_needs_input",
                        "message": "need a decision",
                        "session_id": "sess-abc"
                    }),
                },
            )
            .await?;
        assert_eq!(
            store.get_node(node.id)?.expect("node").liveness,
            NodeLiveness::WaitingForInput
        );

        // Turn-complete returns it to a truthful non-busy Running state.
        service
            .post_harness_event(
                node.id,
                HarnessEventRequest {
                    source: "claude_hook".to_string(),
                    payload: json!({"hook_event_name": "Stop", "session_id": "sess-abc"}),
                },
            )
            .await?;
        assert_eq!(
            store.get_node(node.id)?.expect("node").liveness,
            NodeLiveness::Running
        );

        // The mapped events were stored as harness_event rows.
        let bodies = store.harness_event_bodies(node.id)?;
        let kinds: Vec<&str> = bodies
            .iter()
            .filter_map(|b| b.get("event").and_then(|v| v.as_str()))
            .collect();
        assert!(kinds.contains(&"node.session_started"));
        assert!(kinds.contains(&"node.awaiting_input"));
        assert!(kinds.contains(&"node.turn_complete"));
        Ok(())
    }

    #[tokio::test]
    async fn post_harness_event_rejects_terminal_nodes() -> Result<(), Box<dyn std::error::Error>> {
        let (_workdir, store) = open_test_store();
        let node = insert_active_node(&store, HarnessKind::ClaudeCode);
        store.set_node_liveness(node.id, NodeLiveness::Stopped)?;
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());

        let response = service
            .post_harness_event(
                node.id,
                HarnessEventRequest {
                    source: "claude_hook".to_string(),
                    payload: json!({"hook_event_name": "Stop", "session_id": "s"}),
                },
            )
            .await?;
        assert!(!response.accepted);
        assert_eq!(response.event, None);
        // No harness_event row was recorded for the inert node.
        assert!(store.harness_event_bodies(node.id)?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn statusline_updates_ctx_pct_and_fires_thresholds_once(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (_workdir, store) = open_test_store();
        let node = insert_active_node(&store, HarnessKind::ClaudeCode);
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());

        let post = |used: f64| {
            let service = service.clone();
            let node_id = node.id;
            async move {
                service
                    .post_harness_event(
                        node_id,
                        HarnessEventRequest {
                            source: "claude_statusline".to_string(),
                            payload: json!({
                                "session_id": "line-sess",
                                "context_window": { "used_percentage": used }
                            }),
                        },
                    )
                    .await
            }
        };

        // Crosses 75 -> one ctx_pressure event; ctx_pct reflects harness value.
        let r1 = post(80.0).await?;
        assert_eq!(r1.event.as_deref(), Some("node.ctx_pressure"));
        let node_after = store.get_node(node.id)?.expect("node");
        assert!((node_after.ctx_pct - 0.80).abs() < 0.001);

        // Same threshold again -> no refire.
        let r2 = post(82.0).await?;
        assert_eq!(r2.event, None);

        // Crosses 90 -> fires the 90 threshold only.
        let r3 = post(95.0).await?;
        assert_eq!(r3.event.as_deref(), Some("node.ctx_pressure"));

        let pressure_thresholds: Vec<f64> = store
            .harness_event_bodies(node.id)?
            .iter()
            .filter(|b| b.get("event").and_then(|v| v.as_str()) == Some("node.ctx_pressure"))
            .filter_map(|b| b.get("threshold").and_then(|v| v.as_f64()))
            .collect();
        assert_eq!(pressure_thresholds, vec![75.0, 90.0]);
        Ok(())
    }

    #[test]
    fn event_catalog_is_exactly_the_firable_set() {
        let ids: std::collections::BTreeSet<String> =
            event_catalog().into_iter().map(|entry| entry.id).collect();
        let expected: std::collections::BTreeSet<String> = [
            "graph.spawn",
            "node.session_started",
            "node.turn_complete",
            "node.awaiting_input",
            "node.idle",
            "node.ctx_pressure",
            "node.tool_call",
            "node.session_end",
            "node.exited",
            "node.errored",
            "channel.inbound",
            "schedule.5m",
            "schedule.30m",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(ids, expected);
        // Removed / merged events must not reappear.
        assert!(!ids.contains("node.permission_requested"));
        assert!(!ids.contains("substrate.unreachable"));
        assert!(!ids.contains("schedule.cron"));
    }

    #[tokio::test]
    async fn interrupt_records_event_without_forcing_stopped(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = env_lock();
        let workdir = tempfile::tempdir()?;
        let home = workdir.path().join("home");
        std::fs::create_dir_all(&home)?;
        let workspace = workdir.path().join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        // A harness that ignores SIGINT and stays alive, so a Ctrl-C does not
        // terminate it — mirroring claude, where Ctrl-C cancels the turn only.
        let script_path = workdir.path().join("fake-harness.sh");
        write_executable_script(
            &script_path,
            "#!/bin/sh\ntrap '' INT\nwhile true; do sleep 1; done\n",
        )?;

        let _home = EnvVarGuard::set_var("HOME", &home);
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.harness.codex_command = script_path.display().to_string();
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let response = service
            .create_node(local_create_request_with_workspace(
                "interrupt semantics",
                &workspace.display().to_string(),
            ))
            .await?;
        let node_id = Uuid::parse_str(&response.node_id)?;
        wait_for_liveness(&store, node_id, NodeLiveness::Running).await?;

        service.interrupt_node(node_id).await?;

        // Liveness stays Running (Ctrl-C is not a stop); an interrupt event is
        // recorded, and no node.exited was forced.
        let node = store.get_node(node_id)?.expect("node exists");
        assert_eq!(node.liveness, NodeLiveness::Running);
        let events = store.list_events(node_id)?;
        assert!(events.iter().any(|e| {
            e.kind == NodeEventKind::RemoteCommandReceived
                && e.body.get("action").and_then(|v| v.as_str()) == Some("interrupt")
        }));

        let _ = service.stop_node(node_id).await;
        Ok(())
    }

    #[tokio::test]
    async fn abnormal_exit_marks_failed_and_fires_node_errored(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = env_lock();
        let workdir = tempfile::tempdir()?;
        let home = workdir.path().join("home");
        std::fs::create_dir_all(&home)?;
        let workspace = workdir.path().join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let path = workdir.path().join("asylum.sqlite3").display().to_string();
        let script_path = workdir.path().join("fake-harness.sh");
        write_executable_script(&script_path, "#!/bin/sh\nexit 3\n")?;

        let _home = EnvVarGuard::set_var("HOME", &home);
        let store = Store::open(path)?;
        let mut config = test_app_config();
        config.harness.codex_command = script_path.display().to_string();
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, config);

        let mut hooks = service.hook_engine.subscribe();
        let response = service
            .create_node(local_create_request_with_workspace(
                "abnormal exit",
                &workspace.display().to_string(),
            ))
            .await?;
        let node_id = Uuid::parse_str(&response.node_id)?;

        wait_for_liveness(&store, node_id, NodeLiveness::Failed).await?;

        let mut saw_errored = false;
        for _ in 0..200 {
            match hooks.try_recv() {
                Ok(event) if event.event == "node.errored" && event.node_id == Some(node_id) => {
                    saw_errored = true;
                    break;
                }
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    sleep(Duration::from_millis(10)).await;
                }
                Err(_) => break,
            }
        }
        assert!(saw_errored, "expected node.errored on nonzero exit");
        Ok(())
    }
}
