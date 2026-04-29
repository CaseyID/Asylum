use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use asylum_core::api::{
    AttachResponse, CapabilityListResponse, ClientConfigResponse, CreateNodeRequest,
    GraphGetResponse, HarnessDescriptor, HarnessDescriptorResponse, HarnessListResponse,
    HealthResponse, LaunchPacketResponse, NativeAttachResponse, NodeCreateResponse,
    NodeEventsResponse, NodeInspectResponse, NodeListResponse, Notification, NotificationsResponse,
    RelationshipCreateRequest, RelationshipResponse, RemoteCommandResponse, SendInputRequest,
    SubstrateDescriptor, SubstrateDescriptorResponse, SubstrateHealth, SubstrateListResponse,
    TokenIssueResponse,
};
use asylum_core::capabilities::CapabilityDescriptor;
use asylum_core::capabilities::CapabilityName;
use asylum_core::event::NodeEventKind;
use asylum_core::node::{
    CapabilitySnapshot, GraphRecord, HarnessKind, NodeLiveness, SubstrateKind,
};
use asylum_core::relationship::RelationshipKind;
use asylum_core::security::TokenRequest;
use serde_json::{json, Value as JsonValue};
use uuid::Uuid;

use crate::attach::AttachTokenIssuer;
use crate::auth::{issue_owner_token, AuthMode};
use crate::channels::{
    descriptor_from_row, message_record_from_row, ntfy_inbound, render_template, require_channel,
    seed_builtin_channels, SeedConfig, NTFY_DEFAULT_ID,
};
use crate::channels::ntfy_inbound::NtfyInboundConfig;
use crate::harness::HarnessRegistry;
use crate::hooks::{
    evaluate_filter, event_catalog, firing_record_from_row, rule_from_row, HookEngine, HookEvent,
    SCHEDULE_30M, SCHEDULE_5M,
};
use crate::notifications::send_with_optional_config;
use crate::recipes;
use crate::remote_commands::{ParsedRemoteCommand, RemoteCommandKind};
use crate::storage::Store;
use crate::substrate::loon::{capability_flags_from_health, LoonHealth, LoonSubstrate};
use crate::substrate::{LocalSubstrate, SubstrateContext};
use asylum_core::api::{
    ChannelCreateRequest, ChannelDescriptor, ChannelInboundRequest, ChannelListResponse,
    ChannelMessagesResponse, ChannelTestRequest, ChannelTestResponse, ChannelUpdateRequest,
    ForkNodeRequest, HookAction, HookCreateRequest, HookEventCatalogResponse, HookFiringsResponse,
    HookListResponse, HookRule, HookTestResponse, HookUpdateRequest, RecipeDescriptor,
    RecipeListResponse, RecipeSpawnRequest, RecipeSpawnResponse,
};
use asylum_core::config::{HarnessConfig, LoonConfig};
use asylum_core::node::NodeRecord;

#[derive(Clone)]
pub struct AppConfig {
    pub base_url: String,
    pub bind_addr: String,
    pub transcripts_dir: String,
    pub workspace_recent_limit: usize,
    pub ntfy_server: Option<String>,
    pub ntfy_topic: Option<String>,
    pub ntfy_token: Option<String>,
    pub ntfy_poll_interval_seconds: Option<u64>,
    pub harness: HarnessConfig,
    pub loon: LoonConfig,
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
}

impl CapabilityService {
    pub fn new(store: Store, auth_mode: AuthMode, config: AppConfig) -> Self {
        let issuer = AttachTokenIssuer::new(
            std::env::var("ASYLUM_ATTACH_SECRET").unwrap_or_else(|_| Uuid::new_v4().to_string()),
        );
        let sink_store = store.clone();
        let local_substrate = LocalSubstrate::new(move |node_id, chunk| {
            if let Err(e) = sink_store.append_transcript_chunk(node_id, chunk) {
                tracing::warn!(error = %e, "failed to persist transcript chunk");
            }
        });
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
        let hook_engine = HookEngine::new();
        Self {
            store,
            harnesses: HarnessRegistry::from_config(&config.harness),
            local_substrate: Arc::new(local_substrate),
            loon_substrate,
            auth_mode,
            attach_issuer: Arc::new(issuer),
            config,
            hook_engine,
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
                    Ok(event) => service.process_hook_event(event).await,
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
    ) -> Result<()> {
        self.store
            .insert_channel_message(channel_id, "in", sender, subject, body, replies)?;
        self.post_hook_event(
            "channel.inbound",
            None,
            serde_json::json!({
                "channel_id": channel_id,
                "sender": sender,
                "subject": subject,
                "body": body,
            }),
        );
        Ok(())
    }

    async fn process_hook_event(&self, event: HookEvent) {
        let Ok(rules) = self.store.list_hooks() else {
            return;
        };
        for row in rules {
            let rule = rule_from_row(row);
            if !rule.enabled || rule.event != event.event {
                continue;
            }
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
            if !evaluate_filter(&rule.filter, &payload) {
                continue;
            }
            let outcome = self.execute_hook_actions(&rule, &payload).await;
            let ok = outcome.is_ok();
            let outcome_text = match &outcome {
                Ok(text) => text.clone(),
                Err(error) => error.to_string(),
            };
            let _ = self.store.insert_hook_firing(
                &rule.id,
                &event.event,
                &outcome_text,
                ok,
                &payload.to_string(),
            );
        }
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
                self.send_via_channel(&channel.id, &rendered_title, &rendered_body)
                    .await?;
                Ok(format!("channel:{}", channel.id))
            }
            "spawn" => {
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
            return Ok("transcript.checkpoint:noop".to_string());
        }
        Err(anyhow!("unknown tool target '{target}'"))
    }

    pub async fn send_via_channel(
        &self,
        channel_id: &str,
        title: &str,
        body: &str,
    ) -> Result<bool> {
        let channel = require_channel(&self.store, channel_id)?;
        let sent = if !channel.live {
            false
        } else if channel.kind == "ntfy" {
            self.notify_send(title.to_string(), body.to_string(), None, None, None)
                .await
                .unwrap_or(false)
        } else {
            true
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
            body,
            &[],
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
                "Issue a browser attach URL",
                true,
            ),
            descriptor(
                CapabilityName::NodeAttachNativeTarget,
                "/api/nodes/{id}/attach/native-target",
                "POST",
                "Describe a native attach target",
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
                "List starter recipes",
                true,
            ),
            descriptor(
                CapabilityName::RecipeSpawn,
                "/api/recipes/{id}/spawn",
                "POST",
                "Spawn nodes from a recipe",
                true,
            ),
            descriptor(
                CapabilityName::NodeFork,
                "/api/nodes/{id}/fork",
                "POST",
                "Fork a node",
                true,
            ),
        ];
        CapabilityListResponse { capabilities }
    }

    pub async fn list_nodes(&self) -> NodeListResponse {
        NodeListResponse {
            nodes: self.store.list_nodes().unwrap_or_default(),
        }
    }

    pub async fn inspect_node(&self, id: Uuid) -> Result<NodeInspectResponse> {
        let node = self.store.get_node(id)?.context("node not found")?;
        Ok(NodeInspectResponse { node })
    }

    pub async fn node_events(&self, node_id: Uuid) -> NodeEventsResponse {
        NodeEventsResponse {
            events: self.store.list_events(node_id).unwrap_or_default(),
        }
    }

    pub async fn list_node_events(&self, node_id: Uuid) -> NodeEventsResponse {
        self.node_events(node_id).await
    }

    pub async fn graph(&self) -> GraphGetResponse {
        GraphGetResponse {
            graph: self.store.graph().unwrap_or(GraphRecord {
                nodes: Vec::new(),
                relationships: Vec::new(),
            }),
        }
    }

    pub async fn graph_get(&self) -> GraphGetResponse {
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
            database_path: self.store.path().to_string(),
            database_size_bytes,
            transcripts_dir: self.config.transcripts_dir.clone(),
        }
    }

    pub async fn list_tokens(&self) -> Result<asylum_core::api::TokenListResponse> {
        let tokens = self.store.list_all_tokens()?;
        Ok(asylum_core::api::TokenListResponse { tokens })
    }

    pub async fn rotate_token(
        &self,
        token_id: Uuid,
    ) -> Result<asylum_core::api::TokenRotateResponse> {
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
        let new_token = asylum_core::api::TokenIssueResponse {
            id: issued.token_id.to_string(),
            raw_token: issued.raw_token,
            scope: issued.scope,
            expires_at_epoch_secs: issued.expires_at_epoch_secs,
        };
        Ok(asylum_core::api::TokenRotateResponse {
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
                available: true,
                command: adapter.command().to_string(),
                caps,
            });
        }
        harnesses.sort_by(|a, b| a.id.cmp(&b.id));
        HarnessDescriptorResponse { harnesses }
    }

    pub async fn list_substrate_descriptors(&self) -> SubstrateDescriptorResponse {
        let nodes = self.store.list_nodes().unwrap_or_default();
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
            healthy: true,
            capacity: 0.0,
            nodes: local_nodes,
        }];
        if self.loon_substrate.is_some() {
            let health = self.substrate_health().await;
            let healthy = health.status == "ok";
            let running = health.running_instances;
            let capacity = if running >= 1 {
                f32::min(1.0, running as f32 / 8.0)
            } else {
                0.0
            };
            substrates.push(SubstrateDescriptor {
                id: "loon".to_string(),
                name: "loon".to_string(),
                host: "loon".to_string(),
                healthy,
                capacity,
                nodes: running as u64,
            });
        }
        SubstrateDescriptorResponse { substrates }
    }

    pub async fn substrate_health(&self) -> SubstrateHealth {
        let status = if let Some(loon) = &self.loon_substrate {
            let health = match loon.health().await {
                Ok(h) => h,
                Err(_) => LoonHealth {
                    status: "unavailable".to_string(),
                    running_instances: 0,
                    harness_profiles: vec![],
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
                running_instances: 0,
                harness_profiles: vec!["local-only".to_string()],
            }
        };
        status
    }

    pub async fn recent_workspaces(&self) -> Vec<String> {
        self.store
            .list_recent_workspaces(self.config.workspace_recent_limit)
            .unwrap_or_default()
    }

    pub async fn create_node(&self, request: CreateNodeRequest) -> Result<NodeCreateResponse> {
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
        let mut launch_args = adapter.launch_args().to_vec();
        launch_args.extend(request.launch_args.clone());
        let context = SubstrateContext {
            node_id: node.id,
            harness: harness.clone(),
            command: adapter.command().to_string(),
            args: launch_args,
            workspace: request.workspace.clone(),
            env: vec![],
        };
        match substrate {
            SubstrateKind::Local => {
                self.local_substrate.launch(context).await?;
                self.store
                    .set_node_liveness(node.id, NodeLiveness::Running)?;
            }
            SubstrateKind::Loon => {
                let loon = self
                    .loon_substrate
                    .as_ref()
                    .ok_or_else(|| anyhow!("unsupported substrate"))?;
                let prompt_capabilities = [
                    ("browser_attach", capabilities.browser_attach),
                    ("native_attach", capabilities.native_attach),
                    ("send_input", capabilities.send_input),
                    ("interrupt", capabilities.interrupt),
                    ("stop", capabilities.stop),
                ];
                let prompt = recipes::launch_packet_markdown(
                    &node.id.to_string(),
                    &self.config.base_url,
                    &node.role_hint,
                    &harness.to_string(),
                    &SubstrateKind::Loon.to_string(),
                    &prompt_capabilities,
                    "new Loon-backed node; explicit graph edges are available through Asylum",
                );
                let payload = crate::substrate::loon::LoonContext {
                    node_id: node.id,
                    harness: harness.clone(),
                    command: adapter.command().to_string(),
                    prompt,
                };
                let caps = loom_support_for_harness(loon, &payload.command, &harness).await?;
                if !caps.send_input {
                    return Err(anyhow!("unsupported_on_substrate"));
                }
                let external_id = loon.launch_node(&payload).await?;
                self.store
                    .set_node_external_id(node.id, Some(external_id))?;
                self.store
                    .set_node_liveness(node.id, NodeLiveness::Running)?;
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

    pub async fn create_relationship(
        &self,
        request: RelationshipCreateRequest,
    ) -> Result<asylum_core::relationship::RelationshipRecord> {
        let source = Uuid::parse_str(&request.source_node_id)?;
        let target = Uuid::parse_str(&request.target_node_id)?;
        let kind = parse_relationship_kind(&request.kind)?;
        self.store
            .create_relationship(source, target, kind, request.label)
    }

    pub async fn list_relationships(&self) -> RelationshipResponse {
        let graph = self.store.graph().unwrap_or(GraphRecord {
            nodes: Vec::new(),
            relationships: Vec::new(),
        });
        RelationshipResponse {
            relationships: graph.relationships,
        }
    }

    pub async fn delete_relationship(&self, id: Uuid) -> bool {
        self.store.delete_relationship(id).unwrap_or(false)
    }

    pub async fn send_input(&self, node_id: Uuid, payload: SendInputRequest) -> Result<()> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        self.store.record_event(
            node_id,
            NodeEventKind::InputSent,
            json!({ "text": payload.text }),
        )?;
        match node.substrate {
            SubstrateKind::Local => {
                self.local_substrate
                    .send_input(node_id, &payload.text)
                    .await?
            }
            SubstrateKind::Loon => {
                if let Some(loon) = &self.loon_substrate {
                    let external_id = node.external_id.context("missing loon external id")?;
                    loon.send_input(&external_id, &payload.text).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn interrupt_node(&self, node_id: Uuid) -> Result<()> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        match node.substrate {
            SubstrateKind::Local => self.local_substrate.interrupt(node_id).await?,
            SubstrateKind::Loon => {
                if let Some(loon) = &self.loon_substrate {
                    let external_id = node.external_id.context("missing loon external id")?;
                    loon.interrupt(&external_id).await?;
                }
            }
        }
        self.store
            .set_node_liveness(node_id, NodeLiveness::Stopped)?;
        self.post_hook_event(
            "node.exited",
            Some(node_id),
            json!({"node": {"id": node_id.to_string()}, "reason": "interrupted"}),
        );
        Ok(())
    }

    pub async fn stop_node(&self, node_id: Uuid) -> Result<()> {
        let node = self.store.get_node(node_id)?.context("node not found")?;
        match node.substrate {
            SubstrateKind::Local => self.local_substrate.stop(node_id).await?,
            SubstrateKind::Loon => {
                if let Some(loon) = &self.loon_substrate {
                    if let Some(external) = node.external_id.as_deref() {
                        loon.stop(external).await?;
                    }
                }
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
                    if let (Some(loon), Some(external)) =
                        (self.loon_substrate.as_ref(), node.external_id.as_deref())
                    {
                        loon.archive(external).await?;
                    }
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

    pub async fn attach_browser(&self, node_id: Uuid) -> Result<AttachResponse> {
        self.store.get_node(node_id)?.context("node not found")?;
        let token = self.attach_issuer.issue(node_id, 600)?;
        let fingerprint = &token.raw[..token.raw.len().min(6)];
        self.store.record_event(
            node_id,
            NodeEventKind::AttachIssued,
            json!({ "token_fingerprint": fingerprint }),
        )?;
        Ok(AttachResponse {
            url: format!("{}/attach/{}", self.config.base_url, token.raw),
            expires_in_seconds: 600,
        })
    }

    pub async fn attach_native_target(&self, node_id: Uuid) -> Result<NativeAttachResponse> {
        self.store.get_node(node_id)?.context("node not found")?;
        let mut environment = std::collections::BTreeMap::new();
        environment.insert("ASYLUM_BASE_URL".to_string(), self.config.base_url.clone());
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
            RemoteCommandKind::ApproveDecision => Ok((
                None,
                resolve_remote_decision(&self.store, &args, "approved")?,
            )),
            RemoteCommandKind::DenyDecision => {
                Ok((None, resolve_remote_decision(&self.store, &args, "denied")?))
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

    pub async fn list_notifications(&self) -> NotificationsResponse {
        let notifications = self
            .store
            .list_notifications()
            .unwrap_or_default()
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
        NotificationsResponse { notifications }
    }

    pub async fn mark_notification_read(&self, id: i64) -> Result<()> {
        self.store.mark_notification_read(id)
    }

    pub async fn notify_send(
        &self,
        title: impl AsRef<str>,
        body: impl AsRef<str>,
        server: Option<String>,
        topic: Option<String>,
        token: Option<String>,
    ) -> Result<bool> {
        let configured = asylum_core::config::NtfyConfig {
            server: server.or_else(|| self.config.ntfy_server.clone()),
            topic: topic.or_else(|| self.config.ntfy_topic.clone()),
            token: token.or_else(|| self.config.ntfy_token.clone()),
            poll_interval_seconds: 30,
        };
        if configured.server.is_none() || configured.topic.is_none() {
            return Ok(false);
        }
        send_with_optional_config(Some(&configured), title.as_ref(), body.as_ref()).await?;
        let _ = self.store.insert_channel_message(
            NTFY_DEFAULT_ID,
            "out",
            "asylum",
            title.as_ref(),
            body.as_ref(),
            &[],
        );
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
            .send_via_channel(id, &request.title, &request.body)
            .await?;
        Ok(ChannelTestResponse { sent })
    }

    pub async fn channel_inbound(&self, id: &str, request: ChannelInboundRequest) -> Result<()> {
        require_channel(&self.store, id)?;
        self.record_channel_inbound(
            id,
            &request.sender,
            &request.subject,
            &request.body,
            &request.replies,
        )
    }

    pub async fn list_hooks(&self) -> Result<HookListResponse> {
        let rows = self.store.list_hooks()?;
        let hooks = rows.into_iter().map(rule_from_row).collect();
        Ok(HookListResponse { hooks })
    }

    pub async fn inspect_hook(&self, id: &str) -> Result<HookRule> {
        let row = self
            .store
            .get_hook(id)?
            .ok_or_else(|| anyhow!("hook '{id}' not found"))?;
        Ok(rule_from_row(row))
    }

    pub async fn create_hook(&self, request: HookCreateRequest) -> Result<HookRule> {
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
        Ok(rule_from_row(row))
    }

    pub async fn update_hook(&self, id: &str, request: HookUpdateRequest) -> Result<HookRule> {
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
        Ok(rule_from_row(row))
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
        let rule = rule_from_row(rule_row);
        let nodes = self.store.list_nodes().unwrap_or_default();
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
        let recipes = recipes::starter_recipes()
            .into_iter()
            .map(|recipe| RecipeDescriptor {
                id: recipe.id.to_string(),
                title: recipe.title.to_string(),
                prompt_template: recipe.prompt_template.to_string(),
                kind: if matches!(recipe.id, "spawn-worker-nodes" | "parallel-exploration") {
                    "fanout".to_string()
                } else {
                    "single".to_string()
                },
            })
            .collect();
        RecipeListResponse { recipes }
    }

    pub async fn spawn_recipe(
        &self,
        recipe_id: &str,
        request: RecipeSpawnRequest,
    ) -> Result<RecipeSpawnResponse> {
        let recipe = recipes::starter_recipes()
            .into_iter()
            .find(|recipe| recipe.id == recipe_id)
            .ok_or_else(|| anyhow!("recipe '{recipe_id}' not found"))?;
        let is_fanout = matches!(recipe.id, "spawn-worker-nodes" | "parallel-exploration");
        let description = request
            .description
            .clone()
            .unwrap_or_else(|| recipe.title.to_string());
        let mut node_ids = Vec::new();
        if is_fanout {
            let supervisor = self
                .create_node(asylum_core::api::CreateNodeRequest {
                    harness: request.harness.clone(),
                    substrate: request.substrate.clone(),
                    role_hint: "supervisor".to_string(),
                    workspace: request.workspace.clone(),
                    description: Some(description.clone()),
                    created_by: None,
                    launch_args: Vec::new(),
                })
                .await?;
            let supervisor_id = Uuid::parse_str(&supervisor.node_id)?;
            node_ids.push(supervisor.node_id.clone());
            for _ in 0..2 {
                let worker = self
                    .create_node(asylum_core::api::CreateNodeRequest {
                        harness: request.harness.clone(),
                        substrate: request.substrate.clone(),
                        role_hint: "worker".to_string(),
                        workspace: request.workspace.clone(),
                        description: Some(description.clone()),
                        created_by: None,
                        launch_args: Vec::new(),
                    })
                    .await?;
                let worker_id = Uuid::parse_str(&worker.node_id)?;
                let _ = self.store.create_relationship(
                    supervisor_id,
                    worker_id,
                    RelationshipKind::SpawnedFor,
                    Some(recipe.id.to_string()),
                );
                node_ids.push(worker.node_id);
            }
        } else {
            let role = request.role_hint.unwrap_or_else(|| "worker".to_string());
            let single = self
                .create_node(asylum_core::api::CreateNodeRequest {
                    harness: request.harness.clone(),
                    substrate: request.substrate.clone(),
                    role_hint: role,
                    workspace: request.workspace.clone(),
                    description: Some(description),
                    created_by: None,
                    launch_args: Vec::new(),
                })
                .await?;
            node_ids.push(single.node_id);
        }
        Ok(RecipeSpawnResponse { node_ids })
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
            .create_node(asylum_core::api::CreateNodeRequest {
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

fn parse_relationship_kind(raw: &str) -> Result<RelationshipKind> {
    Ok(match raw {
        "supervises" => RelationshipKind::Supervises,
        "spawned_for" => RelationshipKind::SpawnedFor,
        "user_created" => RelationshipKind::UserCreated,
        "platform_responsibility" => RelationshipKind::PlatformResponsibility,
        _ => return Err(anyhow!("unsupported relationship kind")),
    })
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

fn resolve_remote_decision(
    store: &Store,
    args: &std::collections::HashMap<String, String>,
    status: &str,
) -> Result<serde_json::Value> {
    let decision_id = args
        .get("decision")
        .ok_or_else(|| anyhow!("decision required"))?;
    if !store.resolve_decision(decision_id, status)? {
        return Err(anyhow!("decision not found"));
    }
    Ok(json!({
        "decision": decision_id,
        "status": status,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::hash_token;
    use asylum_core::config::AsylumConfig;
    use std::collections::HashMap;

    fn test_app_config() -> AppConfig {
        let core = AsylumConfig::default();
        AppConfig {
            base_url: core.base_url,
            bind_addr: "127.0.0.1:7717".to_string(),
            transcripts_dir: "/tmp/asylum-test/transcripts".to_string(),
            workspace_recent_limit: core.workspace.recent_limit,
            ntfy_server: core.ntfy.server,
            ntfy_topic: core.ntfy.topic,
            ntfy_token: core.ntfy.token,
            ntfy_poll_interval_seconds: Some(core.ntfy.poll_interval_seconds),
            harness: core.harness,
            loon: core.loon,
        }
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
            matches!(second, Ok(_)),
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

        use asylum_core::security::TokenRequest;
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
        assert!(!obj.contains_key("raw_token"), "raw_token must not be in token list");
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

        use asylum_core::security::TokenRequest;
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
}
