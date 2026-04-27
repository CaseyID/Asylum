use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use asylum_core::api::{
    AttachResponse, CapabilityListResponse, ClientConfigResponse, CreateNodeRequest,
    GraphGetResponse, HarnessListResponse, HealthResponse, LaunchPacketResponse,
    NativeAttachResponse, NodeCreateResponse, NodeEventsResponse, NodeInspectResponse,
    NodeListResponse, Notification, NotificationsResponse, RelationshipCreateRequest,
    RelationshipResponse, SendInputRequest, SubstrateHealth, SubstrateListResponse,
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
use serde_json::json;
use uuid::Uuid;

use crate::attach::AttachTokenIssuer;
use crate::auth::verify_token;
use crate::auth::{issue_owner_token, AuthMode};
use crate::harness::HarnessRegistry;
use crate::notifications::send_with_optional_config;
use crate::recipes;
use crate::storage::Store;
use crate::substrate::loon::{capability_flags_from_health, LoonHealth, LoonSubstrate};
use crate::substrate::{LocalSubstrate, SubstrateContext};

#[derive(Clone)]
pub struct AppConfig {
    pub base_url: String,
    pub workspace_recent_limit: usize,
    pub ntfy_server: Option<String>,
    pub ntfy_topic: Option<String>,
    pub ntfy_token: Option<String>,
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
        let issuer = AttachTokenIssuer::new("asylum-attach-secret");
        let sink_store = store.clone();
        let local_substrate = LocalSubstrate::new(move |node_id, chunk| {
            let _ = sink_store.append_transcript_chunk(node_id, chunk);
        });
        Self {
            store,
            harnesses: HarnessRegistry::default(),
            local_substrate: Arc::new(local_substrate),
            loon_substrate: None,
            auth_mode,
            attach_issuer: Arc::new(issuer),
            config,
        }
    }

    pub async fn capabilities(&self) -> CapabilityListResponse {
        self.list_capability_descriptors().await
    }

    pub async fn list_capability_descriptors(&self) -> CapabilityListResponse {
        let mut capabilities = Vec::new();
        for kind in self.harnesses.iter() {
            capabilities.push(CapabilityDescriptor {
                name: CapabilityName::WorkspaceListRecent,
                path: format!("/api/{}/capabilities", kind.kind()),
                method: "GET".to_string(),
                description: format!("Capabilities for {} harness", kind.kind()),
                available: true,
            });
        }
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
        HarnessListResponse {
            items: vec!["codex".to_string(), "claude_code".to_string()],
        }
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
        let context = SubstrateContext {
            node_id: node.id,
            harness: harness.clone(),
            command: adapter.command().to_string(),
            args: adapter.launch_args().to_vec(),
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
                let payload = crate::substrate::loon::LoonContext {
                    node_id: node.id,
                    harness: harness.clone(),
                    command: adapter.command().to_string(),
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
        let issued = issue_owner_token(&request.name, &request.scope)?;
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
                expected_hashes
                    .iter()
                    .any(|expected| verify_token(token, expected))
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
