use std::env;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use reqwest::{self, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use uuid::Uuid;

use asylum_types::api::{
    ChannelCreateRequest, ChannelDescriptor, ChannelInboundRequest, ChannelListResponse,
    ChannelMessagesResponse, ChannelTestRequest, ChannelTestResponse, ChannelUpdateRequest,
    CreateNodeRequest, DecisionCreateRequest, DecisionListResponse, DecisionRecord,
    DecisionResolveRequest, ForkNodeRequest, GraphGetResponse, HarnessDescriptorResponse,
    HealthResponse, HookCreateRequest, HookEventCatalogResponse, HookFiringsResponse,
    HookListResponse, HookRule, HookTestResponse, LaunchPacketResponse, NativeAttachResponse,
    NodeCreateResponse, NodeEventsResponse, NodeInspectResponse, NodeListResponse,
    NotificationsResponse, RecipeListResponse, RecipeSpawnRequest, RecipeSpawnResponse,
    RelationshipCreateRequest, RelationshipResponse, RemoteCommandRequest, RemoteCommandResponse,
    SendInputRequest, SpawnPeerRequest, SpawnPeerResponse, TokenIssueResponse,
};
use asylum_types::node::NodeRecord;
use asylum_types::relationship::RelationshipRecord;
use asylum_types::security::TokenRequest;

pub struct AsylumClient {
    base_url: String,
    socket_path: Option<PathBuf>,
    token: Option<String>,
    http: reqwest::Client,
}

impl AsylumClient {
    #[allow(dead_code)]
    const DEFAULT_BASE_URL: &'static str = "http://127.0.0.1:7717";

    #[allow(dead_code)]
    pub fn from_env() -> Self {
        let base_url =
            env::var("ASYLUM_BASE_URL").unwrap_or_else(|_| Self::DEFAULT_BASE_URL.to_string());
        let token = env::var("ASYLUM_TOKEN").ok();
        Self::new(base_url, token)
    }

    pub fn new(base_url: impl Into<String>, token: impl Into<Option<String>>) -> Self {
        Self {
            base_url: base_url.into(),
            socket_path: None,
            token: token.into(),
            http: reqwest::Client::new(),
        }
    }

    #[cfg(unix)]
    pub fn new_socket(socket_path: impl AsRef<Path>) -> Result<Self> {
        let socket_path = socket_path.as_ref().to_path_buf();
        let http = reqwest::Client::builder()
            .unix_socket(socket_path.clone())
            .build()
            .context("build Unix-socket daemon client")?;
        Ok(Self {
            base_url: "http://asylum.local".to_string(),
            socket_path: Some(socket_path),
            token: None,
            http,
        })
    }

    #[cfg(not(unix))]
    pub fn new_socket(_socket_path: impl AsRef<Path>) -> Result<Self> {
        Err(anyhow!(
            "Unix-socket daemon client is only supported on Unix platforms"
        ))
    }

    #[allow(dead_code)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn socket_path(&self) -> Option<&Path> {
        self.socket_path.as_deref()
    }

    fn endpoint(&self, path: &str) -> String {
        let trimmed = self.base_url.trim_end_matches('/');
        if path.starts_with('/') {
            format!("{trimmed}{path}")
        } else {
            format!("{trimmed}/{path}")
        }
    }

    fn request_builder(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut request = self.http.request(method, self.endpoint(path));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        request
    }

    async fn parse_response<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("{status}: {body}"));
        }
        serde_json::from_str::<T>(&body)
            .with_context(|| format!("failed to parse response body for status {status}"))
    }

    async fn send_request<T, P>(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: Option<&P>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let mut request = self.request_builder(method, path);
        if let Some(payload) = payload {
            request = request.json(payload);
        }
        let response = request.send().await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Err(anyhow!("unexpected 204 for endpoint {path}"));
        }
        Self::parse_response(response).await
    }

    async fn send_request_no_content(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<()> {
        let mut request = self.request_builder(method, path);
        if let Some(payload) = payload {
            request = request.json(payload);
        }
        let response = request.send().await?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(anyhow!("{status}: {body}"))
    }

    pub async fn list_nodes(&self) -> Result<NodeListResponse> {
        self.send_request(reqwest::Method::GET, "/api/nodes", Option::<&str>::None)
            .await
    }

    pub async fn create_node(&self, request: CreateNodeRequest) -> Result<NodeCreateResponse> {
        self.send_request(reqwest::Method::POST, "/api/nodes", Some(&request))
            .await
    }

    pub async fn fork_node(&self, id: Uuid, request: ForkNodeRequest) -> Result<NodeRecord> {
        let path = format!("/api/nodes/{id}/fork");
        self.send_request(reqwest::Method::POST, &path, Some(&request))
            .await
    }

    pub async fn spawn_peer(
        &self,
        source_id: Uuid,
        request: SpawnPeerRequest,
    ) -> Result<SpawnPeerResponse> {
        let path = format!("/api/nodes/{source_id}/spawn");
        self.send_request(reqwest::Method::POST, &path, Some(&request))
            .await
    }

    pub async fn inspect_node(&self, id: Uuid) -> Result<NodeInspectResponse> {
        let path = format!("/api/nodes/{id}");
        self.send_request(reqwest::Method::GET, &path, Option::<&str>::None)
            .await
    }

    pub async fn graph(&self) -> Result<GraphGetResponse> {
        self.send_request(reqwest::Method::GET, "/api/graph", Option::<&str>::None)
            .await
    }

    pub async fn recent_workspaces(&self) -> Result<Vec<String>> {
        self.send_request(
            reqwest::Method::GET,
            "/api/workspaces/recent",
            Option::<&str>::None,
        )
        .await
    }

    pub async fn system_map(&self) -> Result<GraphGetResponse> {
        self.send_request(
            reqwest::Method::GET,
            "/api/context/system-map",
            Option::<&str>::None,
        )
        .await
    }

    pub async fn launch_packet(&self, id: Uuid) -> Result<LaunchPacketResponse> {
        let path = format!("/api/context/launch-packet/{id}");
        self.send_request(reqwest::Method::GET, &path, Option::<&str>::None)
            .await
    }

    pub async fn create_relationship(
        &self,
        request: RelationshipCreateRequest,
    ) -> Result<RelationshipRecord> {
        self.send_request(reqwest::Method::POST, "/api/relationships", Some(&request))
            .await
    }

    pub async fn list_relationships(&self) -> Result<RelationshipResponse> {
        self.send_request(
            reqwest::Method::GET,
            "/api/relationships",
            Option::<&str>::None,
        )
        .await
    }

    pub async fn remove_relationship(&self, id: Uuid) -> Result<()> {
        let path = format!("/api/relationships/{id}");
        self.send_request_no_content(reqwest::Method::DELETE, &path, Option::<&str>::None)
            .await
    }

    pub async fn list_channels(&self) -> Result<ChannelListResponse> {
        self.send_request(reqwest::Method::GET, "/api/channels", Option::<&str>::None)
            .await
    }

    pub async fn create_channel(&self, request: ChannelCreateRequest) -> Result<ChannelDescriptor> {
        self.send_request(reqwest::Method::POST, "/api/channels", Some(&request))
            .await
    }

    pub async fn inspect_channel(&self, id: &str) -> Result<ChannelDescriptor> {
        let path = format!("/api/channels/{id}");
        self.send_request(reqwest::Method::GET, &path, Option::<&str>::None)
            .await
    }

    pub async fn update_channel(
        &self,
        id: &str,
        request: ChannelUpdateRequest,
    ) -> Result<ChannelDescriptor> {
        let path = format!("/api/channels/{id}");
        self.send_request(reqwest::Method::PATCH, &path, Some(&request))
            .await
    }

    pub async fn delete_channel(&self, id: &str) -> Result<()> {
        let path = format!("/api/channels/{id}");
        self.send_request_no_content(reqwest::Method::DELETE, &path, Option::<&str>::None)
            .await
    }

    pub async fn channel_messages(
        &self,
        id: &str,
        limit: Option<u32>,
    ) -> Result<ChannelMessagesResponse> {
        let path = match limit {
            Some(limit) => format!("/api/channels/{id}/messages?limit={limit}"),
            None => format!("/api/channels/{id}/messages"),
        };
        self.send_request(reqwest::Method::GET, &path, Option::<&str>::None)
            .await
    }

    pub async fn test_channel(
        &self,
        id: &str,
        request: ChannelTestRequest,
    ) -> Result<ChannelTestResponse> {
        let path = format!("/api/channels/{id}/test");
        self.send_request(reqwest::Method::POST, &path, Some(&request))
            .await
    }

    pub async fn inbound_channel(&self, id: &str, request: ChannelInboundRequest) -> Result<()> {
        let path = format!("/api/channels/{id}/inbound");
        self.send_request_no_content(reqwest::Method::POST, &path, Some(&request))
            .await
    }

    pub async fn list_hooks(&self) -> Result<HookListResponse> {
        self.send_request(reqwest::Method::GET, "/api/hooks", Option::<&str>::None)
            .await
    }

    pub async fn create_hook(&self, request: HookCreateRequest) -> Result<HookRule> {
        self.send_request(reqwest::Method::POST, "/api/hooks", Some(&request))
            .await
    }

    pub async fn delete_hook(&self, id: &str) -> Result<()> {
        let path = format!("/api/hooks/{id}");
        self.send_request_no_content(reqwest::Method::DELETE, &path, Option::<&str>::None)
            .await
    }

    pub async fn list_hook_firings(&self, limit: Option<u32>) -> Result<HookFiringsResponse> {
        let path = match limit {
            Some(limit) => format!("/api/hooks/firings?limit={limit}"),
            None => "/api/hooks/firings".to_string(),
        };
        self.send_request(reqwest::Method::GET, &path, Option::<&str>::None)
            .await
    }

    pub async fn hook_event_catalog(&self) -> Result<HookEventCatalogResponse> {
        self.send_request(
            reqwest::Method::GET,
            "/api/hooks/events",
            Option::<&str>::None,
        )
        .await
    }

    pub async fn test_hook(&self, id: &str) -> Result<HookTestResponse> {
        let path = format!("/api/hooks/{id}/test");
        self.send_request(reqwest::Method::POST, &path, Option::<&str>::None)
            .await
    }

    pub async fn list_recipes(&self) -> Result<RecipeListResponse> {
        self.send_request(reqwest::Method::GET, "/api/recipes", Option::<&str>::None)
            .await
    }

    pub async fn spawn_recipe(
        &self,
        id: &str,
        request: RecipeSpawnRequest,
    ) -> Result<RecipeSpawnResponse> {
        let path = format!("/api/recipes/{id}/spawn");
        self.send_request(reqwest::Method::POST, &path, Some(&request))
            .await
    }

    pub async fn send_remote_command(
        &self,
        request: RemoteCommandRequest,
    ) -> Result<RemoteCommandResponse> {
        self.send_request(
            reqwest::Method::POST,
            "/api/remote-commands",
            Some(&request),
        )
        .await
    }

    pub async fn create_decision(&self, request: DecisionCreateRequest) -> Result<DecisionRecord> {
        self.send_request(reqwest::Method::POST, "/api/decisions", Some(&request))
            .await
    }

    pub async fn list_decisions(&self) -> Result<DecisionListResponse> {
        self.send_request(reqwest::Method::GET, "/api/decisions", Option::<&str>::None)
            .await
    }

    pub async fn get_decision(&self, id: &str) -> Result<DecisionRecord> {
        let path = format!("/api/decisions/{id}");
        self.send_request(reqwest::Method::GET, &path, Option::<&str>::None)
            .await
    }

    pub async fn resolve_decision(
        &self,
        id: &str,
        request: DecisionResolveRequest,
    ) -> Result<DecisionRecord> {
        let path = format!("/api/decisions/{id}/resolve");
        self.send_request(reqwest::Method::POST, &path, Some(&request))
            .await
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        self.send_request(reqwest::Method::GET, "/api/health", Option::<&str>::None)
            .await
    }

    pub async fn harness_descriptors(&self) -> Result<HarnessDescriptorResponse> {
        self.send_request(
            reqwest::Method::GET,
            "/api/harness-descriptors",
            Option::<&str>::None,
        )
        .await
    }

    pub async fn is_healthy(&self) -> bool {
        self.health()
            .await
            .map(|health| health.status == "ok")
            .unwrap_or(false)
    }

    pub async fn send_input(&self, id: Uuid, text: impl Into<String>) -> Result<()> {
        let path = format!("/api/nodes/{id}/input");
        let request = SendInputRequest { text: text.into() };
        self.send_request_no_content(reqwest::Method::POST, &path, Some(&request))
            .await
    }

    pub async fn interrupt_node(&self, id: Uuid) -> Result<()> {
        let path = format!("/api/nodes/{id}/interrupt");
        self.send_request_no_content(reqwest::Method::POST, &path, Option::<&str>::None)
            .await
    }

    pub async fn stop_node(&self, id: Uuid) -> Result<()> {
        let path = format!("/api/nodes/{id}/stop");
        self.send_request_no_content(reqwest::Method::POST, &path, Option::<&str>::None)
            .await
    }

    pub async fn archive_node(&self, id: Uuid) -> Result<()> {
        let path = format!("/api/nodes/{id}/archive");
        self.send_request_no_content(reqwest::Method::POST, &path, Option::<&str>::None)
            .await
    }

    pub async fn native_attach_target(&self, id: Uuid) -> Result<NativeAttachResponse> {
        let path = format!("/api/nodes/{id}/attach/native-target");
        self.send_request(reqwest::Method::POST, &path, Option::<&str>::None)
            .await
    }

    pub async fn browser_attach_url(&self, id: Uuid) -> Result<asylum_types::api::AttachResponse> {
        let path = format!("/api/nodes/{id}/attach/browser");
        self.send_request(reqwest::Method::POST, &path, Option::<&str>::None)
            .await
    }

    #[allow(dead_code)]
    pub async fn node_events(&self, id: Uuid) -> Result<NodeEventsResponse> {
        let path = format!("/api/nodes/{id}/events");
        self.send_request(reqwest::Method::GET, &path, Option::<&str>::None)
            .await
    }

    /// Generic JSON request returning a deserialized value. Used by the MCP layer for
    /// capabilities not yet wrapped in dedicated methods.
    pub async fn send_request_json<T: DeserializeOwned, P: Serialize + ?Sized>(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: Option<&P>,
    ) -> Result<T> {
        self.send_request(method, path, payload).await
    }

    /// Generic no-content request. Used by the MCP layer.
    pub async fn send_request_no_content_pub(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<()> {
        self.send_request_no_content(method, path, payload).await
    }

    pub async fn issue_token(&self, request: TokenRequest) -> Result<TokenIssueResponse> {
        self.send_request(reqwest::Method::POST, "/api/tokens", Some(&request))
            .await
    }

    pub async fn notify_send(
        &self,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Result<bool> {
        let path = "/api/notify/send";
        let payload = serde_json::json!({"title": title.into(), "body": body.into()});
        let response: Value = self
            .send_request(reqwest::Method::POST, path, Some(&payload))
            .await?;
        Ok(response
            .get("sent")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false))
    }

    pub async fn list_notifications(&self) -> Result<NotificationsResponse> {
        self.send_request(
            reqwest::Method::GET,
            "/api/notifications",
            Option::<&str>::None,
        )
        .await
    }

    pub async fn mark_notification_read(&self, id: i64) -> Result<()> {
        let path = format!("/api/notifications/{id}/read");
        self.send_request_no_content(reqwest::Method::POST, &path, Option::<&str>::None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_client_uses_builtin_default_url() {
        let client = AsylumClient::new(AsylumClient::DEFAULT_BASE_URL, Option::<String>::None);
        assert_eq!(client.base_url(), "http://127.0.0.1:7717");
    }
}
