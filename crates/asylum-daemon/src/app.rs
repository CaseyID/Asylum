use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use asylum_types::api::{
    ChannelCreateRequest, ChannelInboundRequest, ChannelTestRequest, ChannelUpdateRequest,
    CreateNodeRequest, DecisionCreateRequest, DecisionResolveRequest, ErrorPayload,
    ForkNodeRequest, HarnessEventRequest, HookCreateRequest, HookUpdateRequest,
    LaunchPacketResponse, SendInputRequest, SpawnPeerRequest,
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
use tokio::sync::broadcast::{self, error::RecvError};
use uuid::Uuid;

use crate::auth::hash_token;
use crate::auth::AuthMode;
use crate::capability_service::{init_login_shell_path, AppConfig, CapabilityService};
use crate::remote_commands::{parse_remote_command, RemoteCommandKind};
use crate::storage::Store;
#[cfg(debug_assertions)]
use axum::response::Html;
#[cfg(debug_assertions)]
use axum::routing::get_service;
use futures::{SinkExt, StreamExt};
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
// `cargo build-asylum`
#[folder = "../../cockpit/dist/"]
struct CockpitAssets;

#[cfg(not(debug_assertions))]
const ASSETS_ROUTE: &str = "/assets/{*path}";

const MISSING_COCKPIT_ASSETS_MESSAGE: &str =
    "cockpit assets not present; run `cargo build-cockpit` or `cargo run-asylum` from the source checkout";

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
    // Reconcile persisted liveness against reality BEFORE binding listeners, so
    // no client ever sees an eternal-Running lie left by the previous process.
    service_arc.reconcile_on_boot().await;
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
    // Probe the user's login-shell PATH once before constructing the service.
    // This must happen before any command_available() calls so that binaries
    // in ~/.local/bin or nvm-managed paths are found even when the daemon is
    // started by systemd (which provides a minimal sanitized PATH).
    init_login_shell_path();

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
            autonomy: config.autonomy,
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
        .route(
            "/api/nodes/{id}/harness-event",
            post(api_node_harness_event),
        )
        .route("/api/nodes/{id}/interrupt", post(api_node_interrupt))
        .route("/api/nodes/{id}/resume", post(api_node_resume))
        .route("/api/nodes/{id}/stop", post(api_node_stop))
        .route("/api/nodes/{id}/archive", post(api_node_archive))
        .route("/api/nodes/{id}/spawn", post(api_node_spawn_peer))
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
) -> Result<Json<asylum_types::api::NodeListResponse>, AppError> {
    let response = state
        .service
        .list_nodes()
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_graph(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<asylum_types::api::GraphGetResponse>, AppError> {
    let response = state
        .service
        .graph()
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
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
    let response = state
        .service
        .node_events(id)
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
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

pub async fn api_node_harness_event(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<HarnessEventRequest>,
) -> Result<Json<asylum_types::api::HarnessEventResponse>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let response = state
        .service
        .post_harness_event(id, request)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
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

pub async fn api_node_resume(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    state
        .service
        .resume_node(id)
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

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = %error, node_id = %node_id, "failed to load node events");
            let _ = socket
                .send(Message::Text(
                    format!("failed to load node events: {error}").into(),
                ))
                .await;
            return;
        }
    };

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
    // Live PTY output streams for both Local and Loon nodes (Loon frames flow
    // through the same broadcast as the transcript sink).
    let output = match node.substrate {
        SubstrateKind::Local => service.local_substrate.attach(node_id).await.ok(),
        SubstrateKind::Loon => {
            match (service.loon_substrate.as_ref(), node.external_id.as_deref()) {
                (Some(loon), Some(external_id)) => loon.attach(external_id).await.ok(),
                _ => None,
            }
        }
    };
    let Some(mut output) = output else {
        let _ = socket
            .send(Message::Text(
                "asylum.observe.ws.live_stream_unavailable".into(),
            ))
            .await;
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
) -> Result<Json<asylum_types::api::SubstrateDescriptorResponse>, AppError> {
    let response = state
        .service
        .list_substrate_descriptors()
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_recent_workspaces(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<String>>, AppError> {
    let response = state
        .service
        .recent_workspaces()
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_system_map(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<asylum_types::api::GraphGetResponse>, AppError> {
    let response = state
        .service
        .graph()
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
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
) -> Result<Json<asylum_types::api::RelationshipResponse>, AppError> {
    let response = state
        .service
        .list_relationships()
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
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
) -> Result<Json<asylum_types::api::NotificationsResponse>, AppError> {
    let response = state
        .service
        .list_notifications()
        .await
        .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_notification_read(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<i64>,
) -> StatusCode {
    match state.service.mark_notification_read(id).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(error) if error.to_string().contains("notification not found") => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
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
        Ok(_record) => {
            let html = attach_page_html(&token);
            (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                html,
            )
                .into_response()
        }
        Err(error) => (StatusCode::UNAUTHORIZED, error.to_string()).into_response(),
    }
}

fn attach_page_html(token: &str) -> String {
    // The WS handler sends terminal output as Text frames (raw UTF-8, may contain ANSI
    // escape codes).  It accepts input as either Text or Binary frames — raw bytes that
    // are forwarded directly to the pty / subprocess stdin.  There is no server-side
    // resize protocol, so we only call fit() locally on window resize.
    const TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Asylum terminal</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.css" />
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    html, body { width: 100%; height: 100%; background: #1e1e1e; overflow: hidden; }
    #terminal { width: 100%; height: 100%; }
  </style>
</head>
<body>
  <div id="terminal"></div>
  <script src="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.js"></script>
  <script>
    (function () {
      var token = "__TOKEN__";
      var wsScheme = location.protocol === "https:" ? "wss" : "ws";
      var wsUrl = wsScheme + "://" + location.host + "/api/attach/" + token + "/ws";

      var term = new Terminal({
        cursorBlink: true,
        scrollback: 5000,
        fontFamily: "monospace",
      });
      var fitAddon = new FitAddon.FitAddon();
      term.loadAddon(fitAddon);
      term.open(document.getElementById("terminal"));
      fitAddon.fit();

      var ws = new WebSocket(wsUrl);
      ws.binaryType = "arraybuffer";

      ws.addEventListener("open", function () {
        term.focus();
      });

      // Server sends output as UTF-8 text frames (may include ANSI escape sequences).
      ws.addEventListener("message", function (event) {
        if (typeof event.data === "string") {
          term.write(event.data);
        } else {
          // Binary frame — decode as UTF-8 and write.
          var bytes = new Uint8Array(event.data);
          var text = new TextDecoder("utf-8", { fatal: false }).decode(bytes);
          term.write(text);
        }
      });

      ws.addEventListener("close", function () {
        term.writeln("\r\n[connection closed]");
      });

      ws.addEventListener("error", function () {
        term.writeln("\r\n[connection error]");
      });

      // Forward keystrokes to the server as text frames.
      term.onData(function (data) {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(data);
        }
      });

      // Resize: fit locally; no server-side resize protocol is implemented.
      window.addEventListener("resize", function () {
        fitAddon.fit();
      });
    })();
  </script>
</body>
</html>
"#;
    TEMPLATE.replace("__TOKEN__", token)
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
    if let Err(error) = state.service.require_attachable_node(node.id).await {
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    // Both substrates stream the live PTY over a broadcast; input is routed to
    // the owning substrate inside handle_attach_ws.
    ws.on_upgrade(move |socket| handle_attach_ws(socket, state.service.clone(), node.id))
}

async fn handle_attach_ws(
    mut socket: WebSocket,
    service: crate::capability_service::CapabilityService,
    node_id: Uuid,
) {
    let node = match service.store.get_node(node_id) {
        Ok(Some(node)) => node,
        _ => {
            let _ = socket
                .send(Message::Text("attach failed: node not found".into()))
                .await;
            return;
        }
    };
    let output = match attach_output_for(&service, &node).await {
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
                    if let Err(error) = route_attach_input(&service, &node, text.as_bytes()).await {
                        let _ = send
                            .send(Message::Text(format!("input failed: {error}").into()))
                            .await;
                    }
                }
                Some(Ok(Message::Binary(bytes))) => {
                    match String::from_utf8(bytes.to_vec()) {
                        Ok(text) => {
                            if let Err(error) = route_attach_input(&service, &node, text.as_bytes()).await {
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

/// Resolve the live PTY output broadcast for a node from its owning substrate.
async fn attach_output_for(
    service: &crate::capability_service::CapabilityService,
    node: &asylum_types::node::NodeRecord,
) -> anyhow::Result<broadcast::Receiver<String>> {
    match node.substrate {
        SubstrateKind::Local => service.local_substrate.attach(node.id).await,
        SubstrateKind::Loon => {
            let loon = service
                .loon_substrate
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("loon substrate unavailable"))?;
            let external_id = node
                .external_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing loon external id"))?;
            loon.attach(external_id).await
        }
    }
}

/// Route raw attach input (no appended submit key) to the node's owning
/// substrate PTY.
async fn route_attach_input(
    service: &crate::capability_service::CapabilityService,
    node: &asylum_types::node::NodeRecord,
    bytes: &[u8],
) -> anyhow::Result<()> {
    match node.substrate {
        SubstrateKind::Local => {
            let text = String::from_utf8_lossy(bytes);
            service.local_substrate.send_input_raw(node.id, &text).await
        }
        SubstrateKind::Loon => {
            let loon = service
                .loon_substrate
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("loon substrate unavailable"))?;
            let external_id = node
                .external_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing loon external id"))?;
            loon.send_input_raw(external_id, bytes).await
        }
    }
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
    let response = state.service.hook_test(&id).await.map_err(|err| {
        let status = if err.to_string().contains("not found") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        AppError::new(status, err.to_string())
    })?;
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

pub async fn api_node_spawn_peer(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<SpawnPeerRequest>,
) -> Result<Json<asylum_types::api::SpawnPeerResponse>, AppError> {
    let id = Uuid::parse_str(&id)
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let response = state
        .service
        .spawn_peer(id, request)
        .await
        .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(response))
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
    use super::*;
    #[cfg(debug_assertions)]
    use super::{content_type, normalize_asset_path};
    #[cfg(not(debug_assertions))]
    use super::{content_type, normalize_asset_path, ASSETS_ROUTE};
    use crate::capability_service::{AppConfig, CapabilityService};
    use crate::storage::Store;
    use asylum_types::config::AsylumConfig;
    use rusqlite::Connection;
    use std::sync::Arc;
    use tempfile::tempdir;

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

    fn test_app_config() -> AppConfig {
        let core = AsylumConfig::default();
        AppConfig {
            base_url: core.base_url,
            bind_addr: "127.0.0.1:0".to_string(),
            socket_path: None,
            transcripts_dir: "/tmp/asylum-test-app/transcripts".to_string(),
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

    fn open_with_missing_table(path: &std::path::Path, table: &str) {
        let connection = Connection::open(path).expect("open sqlite");
        connection
            .execute_batch(&format!("DROP TABLE {table};"))
            .expect("drop sqlite table");
    }

    #[tokio::test]
    async fn api_nodes_list_returns_500_when_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        open_with_missing_table(path.as_path(), "nodes");

        let error = api_nodes_list(Extension(state))
            .await
            .expect_err("nodes handler should fail when nodes table is missing");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn api_graph_returns_500_when_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        open_with_missing_table(path.as_path(), "nodes");

        let error = api_graph(Extension(state))
            .await
            .expect_err("graph handler should fail when nodes table is missing");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn api_node_events_returns_500_when_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        open_with_missing_table(path.as_path(), "events");

        let error = api_node_events(Extension(state), Path(Uuid::new_v4().to_string()))
            .await
            .expect_err("node events handler should fail when events table is missing");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn api_list_relationships_returns_500_when_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        open_with_missing_table(path.as_path(), "relationships");

        let error = api_list_relationships(Extension(state))
            .await
            .expect_err("relationship handler should fail when relationship table is missing");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn api_substrate_descriptors_returns_500_when_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        open_with_missing_table(path.as_path(), "nodes");

        let error = api_substrate_descriptors(Extension(state))
            .await
            .expect_err("substrate descriptor handler should fail when nodes table is missing");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn api_recent_workspaces_returns_500_when_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        open_with_missing_table(path.as_path(), "nodes");

        let error = api_recent_workspaces(Extension(state))
            .await
            .expect_err("recent workspaces handler should fail when nodes table is missing");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn api_notifications_returns_500_when_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        open_with_missing_table(path.as_path(), "notifications");

        let error = api_notifications(Extension(state))
            .await
            .expect_err("notifications handler should fail when notifications table is missing");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn api_notification_read_returns_204_for_existing_notification() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store.clone(), AuthMode::Disabled, test_app_config());
        let notification_id = service
            .store
            .insert_notification(None, "status", "Ready", "Node is ready")
            .expect("insert test notification");

        let state = Arc::new(AppState { service });
        let status = api_notification_read(Extension(state), Path(notification_id)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn api_notification_read_returns_404_for_missing_notification() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        let status = api_notification_read(Extension(state), Path(999_999)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_notification_read_returns_500_when_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
        let service = CapabilityService::new(store, AuthMode::Disabled, test_app_config());
        let state = Arc::new(AppState { service });

        open_with_missing_table(path.as_path(), "notifications");
        let status = api_notification_read(Extension(state), Path(1)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn api_hook_test_returns_500_when_node_store_fails() {
        let workdir = tempdir().expect("create test workdir");
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("path")).expect("open store");
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
            .await
            .expect("create hook");

        let state = Arc::new(AppState { service });
        open_with_missing_table(path.as_path(), "nodes");

        let error = api_hook_test(Extension(state), Path(hook.id))
            .await
            .expect_err("hook test handler should fail when nodes table is missing");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
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

    let path = request.uri().path().to_string();

    // Bearer header: validate, then M3 scope-check (a per-node guest token may
    // only act on its own node's path).
    let header_value = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|v| v.to_string());
    if let Some(value) = header_value {
        if state.service.validate_owner_token(Some(&value)) {
            let raw = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .unwrap_or(&value);
            if state.service.scoped_token_authorizes_path(raw, &path) {
                return next.run(request).await;
            }
            return forbidden_cross_node();
        }
    }

    // Browser WebSocket clients cannot set custom headers; accept a ?token= query param.
    let query_token = request.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });
    if let Some(token) = query_token {
        if state.service.validate_owner_token_value(&token) {
            if state.service.scoped_token_authorizes_path(&token, &path) {
                return next.run(request).await;
            }
            return forbidden_cross_node();
        }
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

/// M3: a valid per-node guest token tried to act outside its own node (or on the
/// token-management surface). Authenticated but not authorized.
fn forbidden_cross_node() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorPayload {
            code: "forbidden".to_string(),
            message: "token is scoped to its own node and may not act on this resource".to_string(),
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
