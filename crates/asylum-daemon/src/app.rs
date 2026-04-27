use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use asylum_core::api::{CreateNodeRequest, ErrorPayload, LaunchPacketResponse, SendInputRequest};
use asylum_core::security::TokenRequest;
use axum::extract::ws::Message;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        Extension, Json, Path, State,
    },
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get_service,
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::auth::AuthMode;
use crate::capability_service::{AppConfig, CapabilityService};
use crate::remote_commands::{parse_remote_command, RemoteCommandKind};
use crate::storage::Store;

#[derive(Clone)]
pub struct AppState {
    pub service: CapabilityService,
}

pub async fn serve(bind: SocketAddr, database: String) -> Result<()> {
    let store = Store::open(database)?;
    let auth_mode = AuthMode::Disabled;
    let service = CapabilityService::new(
        store,
        auth_mode,
        AppConfig {
            base_url: format!("http://{bind}"),
            workspace_recent_limit: 50,
            ntfy_server: None,
            ntfy_topic: None,
            ntfy_token: None,
        },
    );

    let state = Arc::new(AppState { service });
    let router = build_router(state.clone());
    println!("Asylum serving on http://{bind}");
    let listener = TcpListener::bind(bind).await?;
    axum::serve(listener, router.into_make_service()).await?;
    Ok(())
}

pub fn build_router(state: Arc<AppState>) -> Router {
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
        .route("/api/tokens", post(api_issue_token))
        .route("/api/tokens/{id}", delete(api_revoke_token))
        .route("/api/harnesses", get(api_harnesses))
        .route("/api/substrates", get(api_substrates))
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
        .route("/api/remote-commands", post(api_remote_commands))
        .route("/api/notify/send", post(api_notify_send))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .nest_service("/assets", get_service(ServeDir::new("cockpit/dist/assets")))
        .route("/attach/{token}", get(api_attach_page))
        .route("/api/attach/{token}/ws", get(api_attach_ws))
        .route("/", get(api_root))
        .merge(protected)
        .layer(axum::Extension(state))
}

pub async fn api_health(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::HealthResponse> {
    Json(state.service.health().await)
}

pub async fn api_capabilities(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::CapabilityListResponse> {
    Json(state.service.capabilities().await)
}

pub async fn api_client_config(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::ClientConfigResponse> {
    Json(state.service.client_config().await)
}

pub async fn api_nodes_list(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::NodeListResponse> {
    Json(state.service.list_nodes().await)
}

pub async fn api_graph(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::GraphGetResponse> {
    Json(state.service.graph().await)
}

pub async fn api_nodes_create(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CreateNodeRequest>,
) -> Result<Json<asylum_core::api::NodeCreateResponse>, AppError> {
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
) -> Result<Json<asylum_core::api::NodeInspectResponse>, AppError> {
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
) -> Result<Json<asylum_core::api::NodeEventsResponse>, AppError> {
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
) -> Result<Json<asylum_core::api::AttachResponse>, AppError> {
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
) -> Result<Json<asylum_core::api::NativeAttachResponse>, AppError> {
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
                break;
            }
        }
    }

    let _ = socket
        .send(Message::Text("asylum.observe.ws.initialized".into()))
        .await;
}

pub async fn api_harnesses(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::HarnessListResponse> {
    Json(state.service.list_harnesses().await)
}

pub async fn api_substrates(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::SubstrateListResponse> {
    let response = state.service.list_substrates().await;
    Json(response)
}

pub async fn api_recent_workspaces(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<Vec<String>> {
    Json(state.service.recent_workspaces().await)
}

pub async fn api_system_map(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::GraphGetResponse> {
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
    Json(request): Json<asylum_core::api::RelationshipCreateRequest>,
) -> Result<Json<asylum_core::relationship::RelationshipRecord>, AppError> {
    let response = state
        .service
        .create_relationship(request)
        .await
        .map_err(|error| AppError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(response))
}

pub async fn api_list_relationships(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::RelationshipResponse> {
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
) -> Result<Json<asylum_core::api::TokenIssueResponse>, AppError> {
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

pub async fn api_notifications(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<asylum_core::api::NotificationsResponse> {
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

pub async fn api_remote_commands(
    Extension(_state): Extension<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> StatusCode {
    let raw = payload
        .get("command")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.as_str())
        .unwrap_or("");
    let command = match parse_remote_command(raw) {
        Ok(command) => command,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let has_required_node_id = matches!(
        command.kind,
        RemoteCommandKind::Attach
            | RemoteCommandKind::SendInput
            | RemoteCommandKind::Interrupt
            | RemoteCommandKind::Stop
    ) && command.node_id.is_none();

    if has_required_node_id {
        return StatusCode::BAD_REQUEST;
    }

    let _ = command.args;
    StatusCode::OK
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
    if state.service.verify_attach_token(&token).is_err() {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    ws.on_upgrade(handle_attach_ws)
}

async fn handle_attach_ws(_socket: WebSocket) {}

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

async fn api_notify_send(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<NotifySendRequest>,
) -> Json<NotifySendResponse> {
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
        .unwrap_or(false);
    Json(NotifySendResponse { sent })
}

pub async fn api_root() -> Html<String> {
    match tokio::fs::read_to_string("cockpit/dist/index.html").await {
        Ok(contents) => Html(contents),
        Err(_) => Html(
            "cockpit assets not present; run `npm --prefix cockpit run build` and serve `cockpit/dist`"
                .to_string(),
        ),
    }
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let has_bearer = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| state.service.validate_owner_token(Some(value)));

    if state.service.auth_mode == AuthMode::Disabled || has_bearer {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorPayload {
                code: "unauthorized".to_string(),
                message: "missing or invalid token".to_string(),
            }),
        )
            .into_response()
    }
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
