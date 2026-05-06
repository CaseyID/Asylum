use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use asylum_types::api::{
    ChannelCreateRequest, ChannelInboundRequest, ChannelTestRequest, ChannelUpdateRequest,
    CreateNodeRequest, DecisionCreateRequest, DecisionResolveRequest, ErrorPayload,
    ForkNodeRequest, HookCreateRequest, HookUpdateRequest, LaunchPacketResponse,
    RecipeSpawnRequest, SendInputRequest,
};
use asylum_types::config::AsylumConfig;
use asylum_types::node::SubstrateKind;
use asylum_types::security::TokenRequest;
use axum::extract::ws::Message;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        Extension, Json, Path, Query, State,
    },
    http::header::AUTHORIZATION,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast::error::RecvError, mpsc, Mutex};
use uuid::Uuid;

use crate::auth::hash_token;
use crate::auth::AuthMode;
use crate::capability_service::{AppConfig, CapabilityService};
use crate::remote_commands::{parse_remote_command, RemoteCommandKind};
use crate::storage::Store;
#[cfg(debug_assertions)]
use axum::response::Html;
#[cfg(debug_assertions)]
use axum::routing::get_service;
use futures::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
#[cfg(not(debug_assertions))]
use rust_embed::RustEmbed;
#[cfg(debug_assertions)]
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub service: CapabilityService,
}

#[cfg(not(debug_assertions))]
#[derive(RustEmbed)]
// In release builds, cockpit assets must be built before compiling this crate.
// Ensure `cockpit/dist` exists by running:
// `npm --prefix cockpit run build`
#[folder = "../../cockpit/dist/"]
struct CockpitAssets;

#[cfg(not(debug_assertions))]
const ASSETS_ROUTE: &str = "/assets/{*path}";

const MISSING_COCKPIT_ASSETS_MESSAGE: &str =
    "cockpit assets not present; run `npm --prefix cockpit run build` and serve `cockpit/dist`";

pub async fn serve(bind: SocketAddr, database: String, config: AsylumConfig) -> Result<()> {
    serve_with_socket(bind, database, None, config).await
}

pub async fn serve_with_socket(
    bind: SocketAddr,
    database: String,
    socket_path: Option<PathBuf>,
    config: AsylumConfig,
) -> Result<()> {
    let state = build_state(bind, database, socket_path.clone(), config)?;
    let service_arc = Arc::new(state.service.clone());
    service_arc.start_background_tasks();

    let tcp_router = build_router_for_transport(state.clone(), true);
    println!("Asylum daemon HTTP listening on http://{bind}");
    let tcp_listener = TcpListener::bind(bind).await?;
    let tcp_server = axum::serve(tcp_listener, tcp_router.into_make_service());

    #[cfg(unix)]
    if let Some(socket_path) = socket_path {
        let socket_listener = bind_unix_socket(&socket_path).await?;
        let socket_router = build_router_for_transport(state.clone(), false);
        println!(
            "Asylum daemon local control listening on {}",
            socket_path.display()
        );
        tokio::select! {
            result = tcp_server => result?,
            result = axum::serve(socket_listener, socket_router.into_make_service()) => result?,
        }
        return Ok(());
    }

    #[cfg(not(unix))]
    if socket_path.is_some() {
        bail!("local control sockets are only supported on Unix platforms");
    }

    tcp_server.await?;
    Ok(())
}

fn build_state(
    bind: SocketAddr,
    database: String,
    socket_path: Option<PathBuf>,
    config: AsylumConfig,
) -> Result<Arc<AppState>> {
    let store = Store::open(database)?;
    // The static config token (from env/flag) has no DB row so we store its hash
    // separately for a direct short-circuit.  All DB-issued tokens are validated
    // on every request via find_token_by_hash, which enforces
    // revoked=0 AND expires_at >= now, so revocation takes effect immediately
    // without a daemon restart.
    let config_token_hash = config.auth.owner_token.as_deref().and_then(|t| {
        if t.is_empty() {
            None
        } else {
            Some(hash_token(t))
        }
    });
    let has_db_tokens = !store.list_active_tokens()?.is_empty();
    let auth_mode = if config.auth.owner_tokens_enabled || config.auth.owner_token.is_some() {
        if config_token_hash.is_none() && !has_db_tokens {
            bail!(
                "owner-token auth is enabled but no active token or ASYLUM_OWNER_TOKEN/--owner-token was provided"
            );
        }
        AuthMode::OwnerToken { config_token_hash }
    } else {
        AuthMode::Disabled
    };
    let base_url = if config.base_url.is_empty() {
        format!("http://{bind}")
    } else {
        config.base_url.clone()
    };
    let transcripts_dir = config
        .harness
        .default_workspace_root
        .as_ref()
        .map(|p| p.join("transcripts").display().to_string())
        .unwrap_or_else(|| {
            // fall back to ~/.asylum/transcripts if no workspace root is configured
            std::env::var("HOME")
                .map(|h| format!("{h}/.asylum/transcripts"))
                .unwrap_or_else(|_| ".asylum/transcripts".to_string())
        });
    let service = CapabilityService::new(
        store,
        auth_mode,
        AppConfig {
            base_url,
            bind_addr: format!("{bind}"),
            socket_path: socket_path.map(|path| path.display().to_string()),
            transcripts_dir,
            workspace_recent_limit: config.workspace.recent_limit,
            ntfy_server: config.ntfy.server,
            ntfy_topic: config.ntfy.topic,
            ntfy_token: config.ntfy.token,
            ntfy_poll_interval_seconds: Some(config.ntfy.poll_interval_seconds),
            harness: config.harness,
            loon: config.loon,
        },
    );

    Ok(Arc::new(AppState { service }))
}

#[cfg(unix)]
async fn bind_unix_socket(path: &FsPath) -> Result<UnixListener> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create socket directory {}", parent.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }

    if path.exists() {
        if UnixStream::connect(path).await.is_ok() {
            bail!("local control socket is already in use: {}", path.display());
        }
        tokio::fs::remove_file(path)
            .await
            .with_context(|| format!("remove stale socket {}", path.display()))?;
    }

    let listener = UnixListener::bind(path)
        .with_context(|| format!("bind local control socket {}", path.display()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(listener)
}

pub fn build_router(state: Arc<AppState>) -> Router {
    build_router_for_transport(state, true)
}

pub fn build_router_for_transport(state: Arc<AppState>, require_auth: bool) -> Router {
    let protected = Router::new()
        .route("/api/capabilities", get(api_capabilities))
        .route("/api/client-config", get(api_client_config))
        .route("/api/health", get(api_health))
        .route("/api/nodes", get(api_nodes_list).post(api_nodes_create))
        .route("/api/nodes/{id}", get(api_node_inspect))
        .route("/api/nodes/{id}/events", get(api_node_events))
        .route("/api/graph", get(api_graph))
        .route("/api/nodes/{id}/input", post(api_node_send_input))
        .route("/api/nodes/{id}/interrupt", post(api_node_interrupt))
        .route("/api/nodes/{id}/stop", post(api_node_stop))
        .route("/api/nodes/{id}/archive", post(api_node_archive))
        .route("/api/nodes/{id}/observe/ws", get(api_node_observe_ws))
        .route(
            "/api/nodes/{id}/attach/browser",
            post(api_node_attach_browser),
        )
        .route(
            "/api/nodes/{id}/attach/native-target",
            post(api_node_attach_native),
        )
        .route("/api/tokens", get(api_tokens_list).post(api_issue_token))
        .route("/api/tokens/{id}", delete(api_revoke_token))
        .route("/api/tokens/{id}/rotate", post(api_token_rotate))
        .route("/api/harnesses", get(api_harnesses))
        .route("/api/substrates", get(api_substrates))
        .route("/api/harness-descriptors", get(api_harness_descriptors))
        .route("/api/substrate-descriptors", get(api_substrate_descriptors))
        .route("/api/workspaces/recent", get(api_recent_workspaces))
        .route("/api/context/system-map", get(api_system_map))
        .route("/api/context/launch-packet/{id}", get(api_launch_packet))
        .route(
            "/api/relationships",
            get(api_list_relationships).post(api_create_relationship),
        )
        .route("/api/relationships/{id}", delete(api_delete_relationship))
        .route("/api/notifications", get(api_notifications))
        .route("/api/notifications/{id}/read", post(api_notification_read))
        .route(
            "/api/decisions",
            get(api_decisions_list).post(api_decision_create),
        )
        .route("/api/decisions/{id}", get(api_decision_get))
        .route("/api/decisions/{id}/resolve", post(api_decision_resolve))
        .route("/api/remote-commands", post(api_remote_commands))
        .route("/api/notify/send", post(api_notify_send))
        .route(
            "/api/channels",
            get(api_channels_list).post(api_channels_create),
        )
        .route(
            "/api/channels/{id}",
            get(api_channel_inspect)
                .patch(api_channel_update)
                .delete(api_channel_delete),
        )
        .route("/api/channels/{id}/messages", get(api_channel_messages))
        .route("/api/channels/{id}/test", post(api_channel_test))
        .route("/api/channels/{id}/inbound", post(api_channel_inbound))
        .route("/api/hooks", get(api_hooks_list).post(api_hooks_create))
        .route(
            "/api/hooks/{id}",
            get(api_hook_inspect)
                .patch(api_hook_update)
                .delete(api_hook_delete),
        )
        .route("/api/hooks/firings", get(api_hook_firings))
        .route("/api/hooks/events", get(api_hook_events))
        .route("/api/hooks/{id}/test", post(api_hook_test))
        .route("/api/recipes", get(api_recipes_list))
        .route("/api/recipes/{id}/spawn", post(api_recipe_spawn))
        .route("/api/nodes/{id}/fork", post(api_node_fork));
    let protected = if require_auth {
        protected.layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
    } else {
        protected
    };

    let mut router = Router::new()
        .route("/attach/{token}", get(api_attach_page))
        .route("/api/attach/{token}/ws", get(api_attach_ws))
        .merge(protected)
        .layer(axum::Extension(state));

    #[cfg(debug_assertions)]
    {
        router = router
            .nest_service("/assets", get_service(ServeDir::new("cockpit/dist/assets")))
            .route("/", get(api_root))
    }

    #[cfg(not(debug_assertions))]
    {
        router = router
            .route(ASSETS_ROUTE, get(api_assets))
            .route("/", get(api_root_embedded))
    }

    router
}

pub async fn api_health(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::HealthResponse> {
    Json(state.service.health().await)
}

pub async fn api_capabilities(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::CapabilityListResponse> {
    Json(state.service.capabilities().await)
}

pub async fn api_client_config(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::ClientConfigResponse> {
    Json(state.service.client_config().await)
}

pub async fn api_nodes_list(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::NodeListResponse> {
    Json(state.service.list_nodes().await)
}

pub async fn api_graph(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::GraphGetResponse> {
    Json(state.service.graph().await)
}

pub async fn api_nodes_create(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CreateNodeRequest>,
) -> Result<Json<asylum_types::api::NodeCreateResponse>, AppError> {
    let response = state
        .service
        .create_node(request)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_node_inspect(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::NodeInspectResponse>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let response = state
        .service
        .inspect_node(id)
        .await
        .map_err(|error| AppError::new(StatusCode::NOT_FOUND, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_node_events(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::NodeEventsResponse>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(state.service.node_events(id).await))
}

pub async fn api_node_send_input(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<SendInputRequest>,
) -> Result<StatusCode, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    state
        .service
        .send_input(id, payload)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn api_node_interrupt(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    state
        .service
        .interrupt_node(id)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn api_node_stop(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    state
        .service
        .stop_node(id)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn api_node_archive(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    state
        .service
        .archive_node(id)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn api_node_attach_browser(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::AttachResponse>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let response = state
        .service
        .attach_browser(id)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_node_attach_native(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::NativeAttachResponse>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let response = state
        .service
        .attach_native_target(id)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_node_observe_ws(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    let id = match Uuid::parse_str(&id) {
        Ok(node_id) => node_id,
        Err(err) => return AppError::new(StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    };

    let service = state.service.clone();
    match service.inspect_node(id).await {
        Ok(_) => ws.on_upgrade(move |socket| handle_node_observe_ws(socket, service, id)),
        Err(_) => AppError::new(StatusCode::NOT_FOUND, "node not found").into_response(),
    }
}

async fn handle_node_observe_ws(
    socket: WebSocket,
    service: crate::capability_service::CapabilityService,
    node_id: Uuid,
) {
    let response = service.node_events(node_id).await;
    let mut socket = socket;

    for event in response.events {
        if let Ok(payload) = serde_json::to_string(&event) {
            if socket.send(Message::Text(payload.into())).await.is_err() {
                return;
            }
        }
    }

    if socket
        .send(Message::Text("asylum.observe.ws.initialized".into()))
        .await
        .is_err()
    {
        return;
    }

    let Ok(Some(node)) = service.store.get_node(node_id) else {
        return;
    };
    if node.substrate != SubstrateKind::Local {
        let _ = socket
            .send(Message::Text(
                "asylum.observe.ws.live_stream_unavailable".into(),
            ))
            .await;
        return;
    }

    let Ok(mut output) = service.local_substrate.attach(node_id).await else {
        return;
    };

    let (mut send, mut recv) = socket.split();

    loop {
        tokio::select! {
            inbound = recv.next() => match inbound {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Ping(payload))) => {
                    let _ = send.send(Message::Pong(payload)).await;
                }
                Some(Ok(_)) | Some(Err(_)) => {}
            },
            output_chunk = output.recv() => match output_chunk {
                Ok(chunk) => {
                    if send.send(Message::Text(chunk.into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
        }
    }
}

pub async fn api_harnesses(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::HarnessListResponse> {
    Json(state.service.list_harnesses().await)
}

pub async fn api_substrates(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::SubstrateListResponse> {
    let response = state.service.list_substrates().await;
    Json(response)
}

pub async fn api_harness_descriptors(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::HarnessDescriptorResponse> {
    Json(state.service.list_harness_descriptors().await)
}

pub async fn api_substrate_descriptors(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::SubstrateDescriptorResponse> {
    Json(state.service.list_substrate_descriptors().await)
}

pub async fn api_recent_workspaces(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<Vec<String>> {
    Json(state.service.recent_workspaces().await)
}

pub async fn api_system_map(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::GraphGetResponse> {
    Json(state.service.graph().await)
}

pub async fn api_launch_packet(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<LaunchPacketResponse>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let response = state
        .service
        .launch_packet(id)
        .await
        .map_err(|error| AppError::new(StatusCode::NOT_FOUND, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_create_relationship(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<asylum_types::api::RelationshipCreateRequest>,
) -> Result<Json<asylum_types::relationship::RelationshipRecord>, AppError> {
    let response = state
        .service
        .create_relationship(request)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_list_relationships(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::RelationshipResponse> {
    Json(state.service.list_relationships().await)
}

pub async fn api_delete_relationship(
    Extension(_state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let Ok(id) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST;
    };
    if _state.service.delete_relationship(id).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn api_issue_token(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<TokenRequest>,
) -> Result<Json<asylum_types::api::TokenIssueResponse>, AppError> {
    let response = state
        .service
        .issue_token(payload)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_revoke_token(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let Ok(id) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST;
    };
    match state.service.revoke_token(id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn api_tokens_list(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<asylum_types::api::TokenListResponse>, AppError> {
    let response = state
        .service
        .list_tokens()
        .await
        .map_err(|e| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(response))
}

pub async fn api_token_rotate(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::TokenRotateResponse>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "invalid token id".to_string()))?;
    let response = state
        .service
        .rotate_token(id)
        .await
        .map_err(|e| AppError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(response))
}

pub async fn api_notifications(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::NotificationsResponse> {
    Json(state.service.list_notifications().await)
}

pub async fn api_notification_read(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<i64>,
) -> StatusCode {
    if state.service.mark_notification_read(id).await.is_ok() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn api_decision_create(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<DecisionCreateRequest>,
) -> Result<Json<asylum_types::api::DecisionRecord>, AppError> {
    let response = state
        .service
        .create_decision(request)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_decisions_list(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<asylum_types::api::DecisionListResponse>, AppError> {
    let response = state
        .service
        .list_decisions()
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_decision_get(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::DecisionRecord>, AppError> {
    let response = state
        .service
        .get_decision(&id)
        .await
        .map_err(|error| AppError::new(StatusCode::NOT_FOUND, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_decision_resolve(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<DecisionResolveRequest>,
) -> Result<Json<asylum_types::api::DecisionRecord>, AppError> {
    let response = state
        .service
        .resolve_decision(&id, request)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_remote_commands(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<asylum_types::api::RemoteCommandResponse>, AppError> {
    let raw = payload
        .get("command")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.as_str())
        .unwrap_or("");
    let command = match parse_remote_command(raw) {
        Ok(command) => command,
        Err(error) => return Err(AppError::new(StatusCode::BAD_REQUEST, error.to_string())),
    };

    let has_required_node_id = matches!(
        command.kind,
        RemoteCommandKind::Attach
            | RemoteCommandKind::SendInput
            | RemoteCommandKind::Interrupt
            | RemoteCommandKind::Stop
    ) && command.node_id.is_none();

    if has_required_node_id {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "command requires node",
        ));
    }

    let token_id = state
        .service
        .token_id_for_raw(&command.token, true)
        .map_err(|error| AppError::new(StatusCode::UNAUTHORIZED, error.to_string()))?;
    let token_id = match token_id {
        Some(token_id) => Some(token_id),
        None => return Err(AppError::new(StatusCode::UNAUTHORIZED, "invalid token")),
    };

    let response = state
        .service
        .execute_remote_command(token_id, command)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_attach_page(
    Extension(state): Extension<Arc<AppState>>,
    Path(token): Path<String>,
) -> Response {
    match state.service.verify_attach_token(&token) {
        Ok(record) => {
            let body = serde_json::json!({ "node_id": record.node_id }).to_string();
            (StatusCode::OK, body).into_response()
        }
        Err(error) => (StatusCode::UNAUTHORIZED, error.to_string()).into_response(),
    }
}

pub async fn api_attach_ws(
    Extension(state): Extension<Arc<AppState>>,
    Path(token): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    let attach_record = match state.service.verify_attach_token(&token) {
        Ok(record) => record,
        Err(error) => {
            return (StatusCode::UNAUTHORIZED, error.to_string()).into_response();
        }
    };

    let node = match state.service.store.get_node(attach_record.node_id) {
        Ok(Some(node)) => node,
        Ok(None) => return (StatusCode::NOT_FOUND, "node not found").into_response(),
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response()
        }
    };
    match node.substrate {
        SubstrateKind::Local => {
            ws.on_upgrade(move |socket| handle_attach_ws(socket, state.service.clone(), node.id))
        }
        SubstrateKind::Loon => {
            let external_id = match node.external_id.as_deref() {
                Some(value) => value.to_string(),
                None => {
                    return (StatusCode::BAD_REQUEST, "missing loon external id").into_response()
                }
            };
            let loon = match state.service.loon_substrate.as_ref() {
                Some(loon) => loon.clone(),
                None => {
                    return (StatusCode::BAD_REQUEST, "loon substrate unavailable").into_response()
                }
            };
            let (command, args, env) = loon.attach_invocation(&external_id);
            ws.on_upgrade(move |socket| handle_command_attach_ws(socket, command, args, env))
        }
    }
}

async fn handle_attach_ws(
    mut socket: WebSocket,
    service: crate::capability_service::CapabilityService,
    node_id: Uuid,
) {
    let output = match service.local_substrate.attach(node_id).await {
        Ok(receiver) => receiver,
        Err(error) => {
            let _ = socket
                .send(Message::Text(format!("attach failed: {error}").into()))
                .await;
            return;
        }
    };

    let (mut send, mut recv) = socket.split();
    let mut output = output;

    loop {
        tokio::select! {
            inbound = recv.next() => match inbound {
                Some(Ok(Message::Text(text))) => {
                    if let Err(error) = service.local_substrate.send_input_raw(node_id, &text).await {
                        let _ = send
                            .send(Message::Text(format!("input failed: {error}").into()))
                            .await;
                    }
                }
                Some(Ok(Message::Binary(bytes))) => {
                    match String::from_utf8(bytes.to_vec()) {
                        Ok(text) => {
                            if let Err(error) = service.local_substrate.send_input_raw(node_id, &text).await {
                                let _ = send
                                    .send(Message::Text(format!("input failed: {error}").into()))
                                    .await;
                            }
                        }
                        Err(_) => {
                            let _ = send
                                .send(Message::Text(
                                    "binary input must be valid UTF-8 for this interface".into(),
                                ))
                                .await;
                        }
                    }
                }
                Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Ping(payload))) => {
                    let _ = send.send(Message::Pong(payload)).await;
                }
                Some(Ok(_)) | Some(Err(_)) | None => break,
            },
            output_chunk = output.recv() => match output_chunk {
                Ok(chunk) => {
                    if send.send(Message::Text(chunk.into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
        }
    }
}

async fn handle_command_attach_ws(
    socket: WebSocket,
    command: std::path::PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
) {
    let pty = match native_pty_system().openpty(PtySize::default()) {
        Ok(pty) => pty,
        Err(error) => {
            send_attach_setup_error(socket, format!("attach failed: {error}")).await;
            return;
        }
    };
    let mut builder = CommandBuilder::new(command);
    for arg in args {
        builder.arg(arg);
    }
    for (key, value) in env {
        builder.env(key, value);
    }
    let child = match pty.slave.spawn_command(builder) {
        Ok(child) => child,
        Err(error) => {
            send_attach_setup_error(socket, format!("attach failed: {error}")).await;
            return;
        }
    };
    let mut reader = match pty.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(error) => {
            send_attach_setup_error(socket, format!("attach failed: {error}")).await;
            return;
        }
    };
    let writer = match pty.master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            send_attach_setup_error(socket, format!("attach failed: {error}")).await;
            return;
        }
    };

    let writer = Arc::new(Mutex::new(writer));
    let (output_tx, mut output_rx) = mpsc::channel::<String>(128);
    tokio::task::spawn_blocking(move || {
        let mut child = child;
        let mut buffer = [0_u8; 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    let chunk = String::from_utf8_lossy(&buffer[..size]).to_string();
                    if output_tx.blocking_send(chunk).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = child.wait();
    });

    let (mut send, mut recv) = socket.split();
    loop {
        tokio::select! {
            inbound = recv.next() => match inbound {
                Some(Ok(Message::Text(text))) => {
                    if write_attach_input(writer.clone(), text.as_bytes()).await.is_err() {
                        let _ = send.send(Message::Text("input failed".into())).await;
                    }
                }
                Some(Ok(Message::Binary(bytes))) => {
                    if write_attach_input(writer.clone(), &bytes).await.is_err() {
                        let _ = send.send(Message::Text("input failed".into())).await;
                    }
                }
                Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Ping(payload))) => {
                    let _ = send.send(Message::Pong(payload)).await;
                }
                Some(Ok(_)) | Some(Err(_)) | None => break,
            },
            output = output_rx.recv() => match output {
                Some(chunk) => {
                    if send.send(Message::Text(chunk.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
        }
    }
}

async fn send_attach_setup_error(mut socket: WebSocket, message: String) {
    let _ = socket.send(Message::Text(message.into())).await;
}

async fn write_attach_input(
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    bytes: &[u8],
) -> std::io::Result<()> {
    let mut writer = writer.lock().await;
    writer.write_all(bytes)?;
    writer.flush()
}

#[derive(Deserialize)]
struct NotifySendRequest {
    title: String,
    body: String,
    server: Option<String>,
    topic: Option<String>,
    token: Option<String>,
}

#[derive(serde::Serialize)]
struct NotifySendResponse {
    sent: bool,
}

#[derive(Deserialize)]
pub struct LimitQuery {
    pub limit: Option<usize>,
}

pub async fn api_channels_list(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<asylum_types::api::ChannelListResponse>, AppError> {
    let response = state
        .service
        .list_channels()
        .await
        .map_err(|err| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_channels_create(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<ChannelCreateRequest>,
) -> Result<Json<asylum_types::api::ChannelDescriptor>, AppError> {
    let response = state
        .service
        .create_channel(request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_channel_inspect(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::ChannelDescriptor>, AppError> {
    let response = state
        .service
        .inspect_channel(&id)
        .await
        .map_err(|err| AppError::new(StatusCode::NOT_FOUND, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_channel_update(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<ChannelUpdateRequest>,
) -> Result<Json<asylum_types::api::ChannelDescriptor>, AppError> {
    let response = state
        .service
        .update_channel(&id, request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_channel_delete(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.service.delete_channel(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => AppError::new(StatusCode::NOT_FOUND, "channel not found").into_response(),
        Err(error) => AppError::new(StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub async fn api_channel_messages(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<asylum_types::api::ChannelMessagesResponse>, AppError> {
    let limit = query.limit.unwrap_or(200).min(1000);
    let response = state
        .service
        .channel_messages(&id, limit)
        .await
        .map_err(|err| AppError::new(StatusCode::NOT_FOUND, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_channel_test(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<ChannelTestRequest>,
) -> Result<Json<asylum_types::api::ChannelTestResponse>, AppError> {
    let response = state
        .service
        .channel_test(&id, request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_channel_inbound(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<ChannelInboundRequest>,
) -> Result<StatusCode, AppError> {
    state
        .service
        .channel_inbound(&id, request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn api_hooks_list(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<asylum_types::api::HookListResponse>, AppError> {
    let response = state
        .service
        .list_hooks()
        .await
        .map_err(|err| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_hooks_create(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<HookCreateRequest>,
) -> Result<Json<asylum_types::api::HookRule>, AppError> {
    let response = state
        .service
        .create_hook(request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_hook_inspect(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::HookRule>, AppError> {
    let response = state
        .service
        .inspect_hook(&id)
        .await
        .map_err(|err| AppError::new(StatusCode::NOT_FOUND, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_hook_update(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<HookUpdateRequest>,
) -> Result<Json<asylum_types::api::HookRule>, AppError> {
    let response = state
        .service
        .update_hook(&id, request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_hook_delete(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    match state.service.delete_hook(&id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn api_hook_firings(
    Extension(state): Extension<Arc<AppState>>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<asylum_types::api::HookFiringsResponse>, AppError> {
    let limit = query.limit.unwrap_or(200).min(1000);
    let response = state
        .service
        .list_hook_firings(limit)
        .await
        .map_err(|err| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_hook_events(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::HookEventCatalogResponse> {
    Json(state.service.hook_event_catalog().await)
}

pub async fn api_hook_test(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<asylum_types::api::HookTestResponse>, AppError> {
    let response = state
        .service
        .hook_test(&id)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_recipes_list(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_types::api::RecipeListResponse> {
    Json(state.service.list_recipes().await)
}

pub async fn api_recipe_spawn(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<RecipeSpawnRequest>,
) -> Result<Json<asylum_types::api::RecipeSpawnResponse>, AppError> {
    let response = state
        .service
        .spawn_recipe(&id, request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(response))
}

pub async fn api_node_fork(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<ForkNodeRequest>,
) -> Result<Json<asylum_types::node::NodeRecord>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let node = state
        .service
        .fork_node(id, request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(node))
}

async fn api_notify_send(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<NotifySendRequest>,
) -> Result<Json<NotifySendResponse>, AppError> {
    let sent = state
        .service
        .notify_send(
            payload.title,
            payload.body,
            payload.server,
            payload.topic,
            payload.token,
        )
        .await
        .map_err(|err| AppError::new(StatusCode::SERVICE_UNAVAILABLE, err.to_string()))?;
    Ok(Json(NotifySendResponse { sent }))
}

#[cfg(debug_assertions)]
pub async fn api_root() -> Html<String> {
    match tokio::fs::read_to_string("cockpit/dist/index.html").await {
        Ok(contents) => Html(contents),
        Err(_) => Html(MISSING_COCKPIT_ASSETS_MESSAGE.to_string()),
    }
}

#[cfg(not(debug_assertions))]
pub async fn api_root_embedded() -> impl IntoResponse {
    match CockpitAssets::get("index.html") {
        Some(file) => (
            StatusCode::OK,
            [("Content-Type", "text/html")],
            file.data.to_vec(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, MISSING_COCKPIT_ASSETS_MESSAGE).into_response(),
    }
}

#[cfg(not(debug_assertions))]
async fn api_assets(Path(path): Path<String>) -> Response {
    let path = match normalize_asset_path(&path) {
        Some(path) => path,
        None => return (StatusCode::NOT_FOUND, "asset not found").into_response(),
    };

    match CockpitAssets::get(&format!("assets/{path}")) {
        Some(file) => (
            StatusCode::OK,
            [("Content-Type", content_type(&path))],
            file.data.to_vec(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "asset not found").into_response(),
    }
}

#[cfg(any(test, not(debug_assertions)))]
fn normalize_asset_path(path: &str) -> Option<String> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let mut segments: Vec<&str> = Vec::with_capacity(4);
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        if segment.contains('\\') {
            return None;
        }
        segments.push(segment);
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments.join("/"))
    }
}

#[cfg(any(test, not(debug_assertions)))]
fn content_type(path: &str) -> &'static str {
    let (_, extension) = path.rsplit_once('.').unwrap_or(("", ""));
    match extension.to_ascii_lowercase().as_str() {
        "html" => "text/html",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript",
        "mjs" => "application/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    #[cfg(debug_assertions)]
    use super::{content_type, normalize_asset_path};
    #[cfg(not(debug_assertions))]
    use super::{content_type, normalize_asset_path, ASSETS_ROUTE};

    #[test]
    fn normalize_asset_path_rejects_traversal_and_empty_paths() {
        assert_eq!(normalize_asset_path(""), None);
        assert_eq!(normalize_asset_path("../index.html"), None);
        assert_eq!(normalize_asset_path("dir/../../etc"), None);
    }

    #[test]
    fn normalize_asset_path_cleans_redundant_segments() {
        assert_eq!(
            normalize_asset_path("/assets/./bundle.css"),
            Some("assets/bundle.css".to_string())
        );
    }

    #[test]
    fn content_type_guesses_common_extensions() {
        assert_eq!(content_type("index.html"), "text/html");
        assert_eq!(content_type("styles/main.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("app.js"), "application/javascript");
        assert_eq!(content_type("image.png"), "image/png");
        assert_eq!(content_type("docs/README.txt"), "text/plain; charset=utf-8");
        assert_eq!(content_type("bundle.unknown"), "application/octet-stream");
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_assets_route_pattern_uses_catch_all() {
        assert_eq!(ASSETS_ROUTE, "/assets/{*path}");
    }
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if state.service.auth_mode == AuthMode::Disabled {
        return next.run(request).await;
    }

    let has_bearer = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| state.service.validate_owner_token(Some(value)));

    if has_bearer {
        return next.run(request).await;
    }

    // Browser WebSocket clients cannot set custom headers; accept a ?token= query param.
    let has_query_token = request
        .uri()
        .query()
        .and_then(|query| {
            url::form_urlencoded::parse(query.as_bytes())
                .find(|(key, _)| key == "token")
                .map(|(_, value)| value.into_owned())
        })
        .is_some_and(|token| state.service.validate_owner_token_value(&token));

    if has_query_token {
        return next.run(request).await;
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorPayload {
            code: "unauthorized".to_string(),
            message: "missing or invalid token".to_string(),
        }),
    )
        .into_response()
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    body: String,
}

impl AppError {
    pub fn new(status: StatusCode, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status;
        let body = Json(json!({
            "code": self.status.as_u16(),
            "message": self.body,
        }));
        (status, body).into_response()
    }
}
