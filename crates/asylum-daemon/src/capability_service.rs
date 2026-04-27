use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use asylum_core::api::{
    AttachResponse, CapabilityListResponse, ClientConfigResponse, CreateNodeRequest,
    GraphGetResponse, HarnessListResponse, HealthResponse, LaunchPacketResponse,
    NativeAttachResponse, NodeCreateResponse, NodeEventsResponse, NodeInspectResponse,
    NodeListResponse, Notification, NotificationsResponse, RelationshipCreateRequest,
    RelationshipResponse, RemoteCommandResponse, SendInputRequest, SubstrateHealth,
    SubstrateListResponse, TokenIssueResponse,
};
use asylum_core::capabilities::CapabilityDescriptor;
use asylum_core::capabilities::CapabilityName;
use asylum_core::event::NodeEventKind;
use asylum_core::node::{
    CapabilitySnapshot, GraphRecord, HarnessKind, NodeLiveness, SubstrateKind,
};
use asylum_core::relationship::RelationshipKind;
use asylum_core::security::TokenRequest;
use serde_json::json;
use uuid::Uuid;

use crate::attach::AttachTokenIssuer;
use crate::auth::{issue_owner_token, AuthMode};
use crate::harness::HarnessRegistry;
use crate::notifications::send_with_optional_config;
use crate::recipes;
use crate::remote_commands::{ParsedRemoteCommand, RemoteCommandKind};
use crate::storage::Store;
use crate::substrate::loon::{capability_flags_from_health, LoonHealth, LoonSubstrate};
use crate::substrate::{LocalSubstrate, SubstrateContext};
use asylum_core::config::{HarnessConfig, LoonConfig};

#[derive(Clone)]
pub struct AppConfig {
    pub base_url: String,
    pub workspace_recent_limit: usize,
    pub ntfy_server: Option<String>,
    pub ntfy_topic: Option<String>,
    pub ntfy_token: Option<String>,
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
}

impl CapabilityService {
    pub fn new(store: Store, auth_mode: AuthMode, config: AppConfig) -> Self {
        let issuer = AttachTokenIssuer::new(
            std::env::var("ASYLUM_ATTACH_SECRET").unwrap_or_else(|_| Uuid::new_v4().to_string()),
        );
        let sink_store = store.clone();
        let local_substrate = LocalSubstrate::new(move |node_id, chunk| {
            let _ = sink_store.append_transcript_chunk(node_id, chunk);
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
        Self {
            store,
            harnesses: HarnessRegistry::from_config(&config.harness),
            local_substrate: Arc::new(local_substrate),
            loon_substrate,
            auth_mode,
            attach_issuer: Arc::new(issuer),
            config,
        }
    }

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
        HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
        }
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
        Ok(())
    }

    pub async fn archive_node(&self, node_id: Uuid) -> Result<()> {
        if let Some(node) = self.store.get_node(node_id)? {
            if let (SubstrateKind::Loon, Some(loon), Some(external)) = (
                node.substrate,
                self.loon_substrate.as_ref(),
                node.external_id.as_deref(),
            ) {
                loon.archive(external).await?;
            }
        }
        self.store
            .set_node_liveness(node_id, NodeLiveness::Archived)
    }

    pub async fn attach_browser(&self, node_id: Uuid) -> Result<AttachResponse> {
        self.store.get_node(node_id)?.context("node not found")?;
        let token = self.attach_issuer.issue(node_id, 600)?;
        self.store.record_event(
            node_id,
            NodeEventKind::AttachIssued,
            json!({ "token": token.raw }),
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
            AuthMode::OwnerToken { expected_hashes } => expected_hashes
                .iter()
                .any(|expected| expected == &token_hash),
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
        Ok(true)
    }

    pub fn validate_owner_token(&self, header: Option<&str>) -> bool {
        match self.auth_mode {
            AuthMode::Disabled => true,
            AuthMode::OwnerToken {
                ref expected_hashes,
            } => {
                let Some(value) = header else {
                    return false;
                };
                let token = value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
                    .unwrap_or(value);
                let hash = crate::auth::hash_token(token);
                if expected_hashes.iter().any(|expected| expected == &hash) {
                    return true;
                }
                self.store
                    .find_token_by_hash(&hash)
                    .ok()
                    .and_then(|value| value.map(|_| ()))
                    .is_some()
            }
        }
    }

    pub fn attach_issuer_clone(&self) -> Arc<AttachTokenIssuer> {
        self.attach_issuer.clone()
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
            workspace_recent_limit: core.workspace.recent_limit,
            ntfy_server: core.ntfy.server,
            ntfy_topic: core.ntfy.topic,
            ntfy_token: core.ntfy.token,
            harness: core.harness,
            loon: core.loon,
        }
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
                expected_hashes: vec![hash_token("bootstrap")],
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
}
