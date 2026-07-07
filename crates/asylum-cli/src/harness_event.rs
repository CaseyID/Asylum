//! CLI bridge for `asylum harness-event <source>`.
//!
//! Invoked by injected Claude Code hooks / statusline and by Codex's `notify`
//! config from inside a running node's environment. It forwards the verbatim
//! harness JSON to the daemon's `POST /api/nodes/{id}/harness-event` endpoint;
//! all interpretation of the payload happens daemon-side (see W1). This module
//! stays dumb plumbing: parse the envelope, forward bytes, never block or fail
//! a harness session.
//!
//! The core logic is split into small, pure/testable pieces
//! (`build_request`, `render_statusline`, `resolve_target`, `dispatch`) so the
//! contract can be verified without a live daemon. The shipped entry points
//! (`run_claude_hook`, `run_claude_statusline`, `run_codex_notify`) always
//! return `()` and never propagate an error: a failing bridge must never break
//! or block a harness session.

use std::env;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use uuid::Uuid;

use crate::client::AsylumClient;
use crate::runtime::RuntimePaths;
use asylum_types::api::{HarnessEventRequest, HarnessEventResponse};

/// Short connect/read timeout for the bridge: hooks run inside async Claude
/// hook timeouts and the statusline runs after every assistant message, so
/// this must fail fast rather than retry or hang.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Fallback base URL when only `ASYLUM_TOKEN` (not `ASYLUM_BASE_URL`) is set.
/// Mirrors `AsylumClient::DEFAULT_BASE_URL`.
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:7717";

/// Which harness-native channel produced the payload. Maps 1:1 to the
/// `source` values the W1 daemon endpoint dispatches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    ClaudeHook,
    ClaudeStatusline,
    CodexNotify,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::ClaudeHook => "claude_hook",
            Source::ClaudeStatusline => "claude_statusline",
            Source::CodexNotify => "codex_notify",
        }
    }
}

/// Pure request-builder: forwards `payload` verbatim, tagged with `source`.
/// No interpretation of the payload happens here or anywhere in the CLI.
pub fn build_request(source: Source, payload: Value) -> HarnessEventRequest {
    HarnessEventRequest {
        source: source.as_str().to_string(),
        payload,
    }
}

/// Where the bridge should send the request: a local Unix socket (primary,
/// no token required — the daemon's socket transport is unauthenticated by
/// design, matching `asylum mcp`'s injected env) or an HTTP base URL with a
/// bearer token (fallback, for future Loon guests where the socket can't
/// cross the VM boundary).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientTarget {
    Socket(PathBuf),
    Http {
        base_url: String,
        token: Option<String>,
    },
}

/// Pure precedence resolver: socket wins whenever `ASYLUM_SOCKET_PATH` is
/// set; otherwise fall back to HTTP if either `ASYLUM_BASE_URL` or
/// `ASYLUM_TOKEN` is set; otherwise fall back to the default local socket
/// path. Mirrors `cli::runtime_client`'s precedence so the bridge resolves
/// the daemon the same way the rest of the CLI does.
pub fn resolve_target(
    socket_path: Option<String>,
    base_url: Option<String>,
    token: Option<String>,
    default_socket_path: PathBuf,
) -> ClientTarget {
    if let Some(socket_path) = socket_path {
        return ClientTarget::Socket(PathBuf::from(socket_path));
    }
    if base_url.is_some() || token.is_some() {
        return ClientTarget::Http {
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            token,
        };
    }
    ClientTarget::Socket(default_socket_path)
}

fn resolve_target_from_env(default_socket_path: PathBuf) -> ClientTarget {
    resolve_target(
        env::var("ASYLUM_SOCKET_PATH").ok(),
        env::var("ASYLUM_BASE_URL").ok(),
        env::var("ASYLUM_TOKEN").ok(),
        default_socket_path,
    )
}

fn build_client(target: &ClientTarget) -> Result<AsylumClient, String> {
    match target {
        ClientTarget::Socket(path) => {
            AsylumClient::new_socket_with_timeout(path, REQUEST_TIMEOUT).map_err(|e| e.to_string())
        }
        ClientTarget::Http { base_url, token } => {
            AsylumClient::new_with_timeout(base_url.clone(), token.clone(), REQUEST_TIMEOUT)
                .map_err(|e| e.to_string())
        }
    }
}

/// Render the claude statusline's stdout line from the raw statusline JSON.
/// Dumb formatting only, with graceful fallbacks when fields are missing —
/// this must never fail or panic, since the statusline command's stdout
/// becomes the visible Claude Code status bar text.
pub fn render_statusline(payload: &Value) -> String {
    let model = payload
        .get("model")
        .and_then(|m| {
            m.get("display_name")
                .and_then(Value::as_str)
                .or_else(|| m.as_str())
        })
        .unwrap_or("claude");

    let used_percentage = payload
        .get("context_window")
        .and_then(|c| c.get("used_percentage"))
        .and_then(Value::as_f64);

    match used_percentage {
        Some(pct) => format!("{model} | ctx {}%", format_percentage(pct)),
        None => model.to_string(),
    }
}

fn format_percentage(pct: f64) -> String {
    if (pct - pct.trunc()).abs() < f64::EPSILON {
        format!("{}", pct as i64)
    } else {
        format!("{pct:.1}")
    }
}

/// Resolve the target node id from `ASYLUM_NODE_ID`. Kept as an explicit
/// `Option<String>` parameter (rather than reading the env directly) so the
/// precedence/failure logic is testable without mutating process env.
fn resolve_node_id(node_id_env: Option<String>) -> Result<Uuid, String> {
    let raw = node_id_env.ok_or_else(|| "ASYLUM_NODE_ID is not set".to_string())?;
    Uuid::parse_str(&raw).map_err(|err| format!("invalid ASYLUM_NODE_ID {raw:?}: {err}"))
}

/// The testable core: resolve the node id, build a client for `target`, and
/// POST the request. Returns `Err(String)` (never panics) so tests can assert
/// on the failure path without needing a live daemon.
async fn dispatch(
    node_id_env: Option<String>,
    target: ClientTarget,
    source: Source,
    payload: Value,
) -> Result<HarnessEventResponse, String> {
    let node_id = resolve_node_id(node_id_env)?;
    let client = build_client(&target)?;
    let request = build_request(source, payload);
    client
        .post_harness_event(node_id, request)
        .await
        .map_err(|err| err.to_string())
}

fn read_stdin_json() -> Result<Value, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|err| format!("failed to read stdin: {err}"))?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        return Err("empty stdin".to_string());
    }
    serde_json::from_str(trimmed).map_err(|err| format!("invalid JSON on stdin: {err}"))
}

/// `asylum harness-event claude-hook`: reads the hook payload JSON from
/// stdin and forwards it. Always exits 0 (returns `()`); failures are logged
/// to stderr only, never stdout, and never block the calling hook.
pub async fn run_claude_hook(paths: &RuntimePaths) {
    let payload = match read_stdin_json() {
        Ok(payload) => payload,
        Err(err) => {
            eprintln!("asylum harness-event claude-hook: {err}");
            return;
        }
    };

    let node_id_env = env::var("ASYLUM_NODE_ID").ok();
    let target = resolve_target_from_env(paths.socket_path());
    if let Err(err) = dispatch(node_id_env, target, Source::ClaudeHook, payload).await {
        eprintln!("asylum harness-event claude-hook: {err}");
    }
}

/// `asylum harness-event claude-statusline`: reads the statusline payload
/// JSON from stdin, forwards it as telemetry, and ALWAYS prints exactly one
/// status line to stdout afterward (Claude Code renders this command's
/// stdout as the status line text). Errors go to stderr only, never stdout.
pub async fn run_claude_statusline(paths: &RuntimePaths) {
    let payload = match read_stdin_json() {
        Ok(payload) => payload,
        Err(err) => {
            eprintln!("asylum harness-event claude-statusline: {err}");
            Value::Null
        }
    };

    let line = render_statusline(&payload);

    let node_id_env = env::var("ASYLUM_NODE_ID").ok();
    let target = resolve_target_from_env(paths.socket_path());
    if let Err(err) = dispatch(node_id_env, target, Source::ClaudeStatusline, payload).await {
        eprintln!("asylum harness-event claude-statusline: {err}");
    }

    println!("{line}");
}

/// `asylum harness-event codex-notify <payload>`: Codex appends the notify
/// JSON as a single trailing argv element and nulls stdin/stdout/stderr, so
/// the payload is read from argv (never stdin) and nothing is printed.
/// stderr writes below are harmless no-ops under codex (fd is null) but keep
/// the same failure-logging shape as the other two sources for consistency.
pub async fn run_codex_notify(payload_arg: Option<String>, paths: &RuntimePaths) {
    let raw = match payload_arg {
        Some(raw) => raw,
        None => {
            eprintln!("asylum harness-event codex-notify: missing payload argument");
            return;
        }
    };
    let payload = match serde_json::from_str::<Value>(&raw) {
        Ok(payload) => payload,
        Err(err) => {
            eprintln!("asylum harness-event codex-notify: invalid JSON argv: {err}");
            return;
        }
    };

    let node_id_env = env::var("ASYLUM_NODE_ID").ok();
    let target = resolve_target_from_env(paths.socket_path());
    if let Err(err) = dispatch(node_id_env, target, Source::CodexNotify, payload).await {
        eprintln!("asylum harness-event codex-notify: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // --- build_request: exact POST body shape, forwarded verbatim ---

    #[test]
    fn build_request_forwards_stop_payload_verbatim() {
        let payload = json!({
            "hook_event_name": "Stop",
            "session_id": "sess-1",
            "transcript_path": "/tmp/t.jsonl",
            "cwd": "/work"
        });
        let request = build_request(Source::ClaudeHook, payload.clone());
        assert_eq!(request.source, "claude_hook");
        assert_eq!(request.payload, payload);
    }

    #[test]
    fn build_request_forwards_notification_payload_verbatim() {
        let payload = json!({
            "hook_event_name": "Notification",
            "type": "permission_prompt",
            "message": "may I run rm -rf /tmp/scratch?",
            "session_id": "sess-2"
        });
        let request = build_request(Source::ClaudeHook, payload.clone());
        assert_eq!(request.source, "claude_hook");
        assert_eq!(request.payload, payload);
    }

    #[test]
    fn build_request_forwards_session_start_payload_verbatim() {
        let payload = json!({
            "hook_event_name": "SessionStart",
            "source": "startup",
            "model": "claude-opus",
            "session_id": "sess-3"
        });
        let request = build_request(Source::ClaudeHook, payload.clone());
        assert_eq!(request.source, "claude_hook");
        assert_eq!(request.payload, payload);
    }

    #[test]
    fn build_request_tags_codex_notify_source() {
        let payload = json!({
            "type": "agent-turn-complete",
            "thread-id": "6a1f-thread",
            "turn-id": "turn-9"
        });
        let request = build_request(Source::CodexNotify, payload.clone());
        assert_eq!(request.source, "codex_notify");
        assert_eq!(request.payload, payload);
    }

    #[test]
    fn build_request_tags_claude_statusline_source() {
        let payload = json!({ "session_id": "sess-line" });
        let request = build_request(Source::ClaudeStatusline, payload.clone());
        assert_eq!(request.source, "claude_statusline");
        assert_eq!(request.payload, payload);
    }

    // --- codex-notify argv parsing ---

    #[test]
    fn codex_notify_argv_parses_into_forwarded_payload() {
        let argv = r#"{"type":"agent-turn-complete","thread-id":"6a1f-thread","turn-id":"turn-9","cwd":"/work","input-messages":["do the thing"],"last-assistant-message":"did the thing"}"#;
        let payload: Value = serde_json::from_str(argv).expect("codex argv is valid JSON");
        assert_eq!(payload["thread-id"], json!("6a1f-thread"));
        let request = build_request(Source::CodexNotify, payload);
        assert_eq!(request.source, "codex_notify");
        assert_eq!(
            request.payload["last-assistant-message"],
            json!("did the thing")
        );
    }

    #[test]
    fn codex_notify_rejects_malformed_argv_without_panicking() {
        let argv = "not json";
        let result: Result<Value, _> = serde_json::from_str(argv);
        assert!(result.is_err());
    }

    // --- claude-statusline rendering: full fields and graceful fallbacks ---

    #[test]
    fn render_statusline_full_fields() {
        let payload = json!({
            "model": { "id": "claude-opus-4-5", "display_name": "Opus 4.5" },
            "context_window": { "used_percentage": 42.0, "remaining_percentage": 58.0 }
        });
        assert_eq!(render_statusline(&payload), "Opus 4.5 | ctx 42%");
    }

    #[test]
    fn render_statusline_rounds_fractional_percentage() {
        let payload = json!({
            "model": { "display_name": "Sonnet 5" },
            "context_window": { "used_percentage": 12.5 }
        });
        assert_eq!(render_statusline(&payload), "Sonnet 5 | ctx 12.5%");
    }

    #[test]
    fn render_statusline_missing_context_window_omits_ctx_clause() {
        let payload = json!({ "model": { "display_name": "Opus 4.5" } });
        assert_eq!(render_statusline(&payload), "Opus 4.5");
    }

    #[test]
    fn render_statusline_missing_model_falls_back() {
        let payload = json!({ "context_window": { "used_percentage": 90.0 } });
        assert_eq!(render_statusline(&payload), "claude | ctx 90%");
    }

    #[test]
    fn render_statusline_handles_plain_string_model() {
        let payload = json!({
            "model": "claude-opus",
            "context_window": { "used_percentage": 5.0 }
        });
        assert_eq!(render_statusline(&payload), "claude-opus | ctx 5%");
    }

    #[test]
    fn render_statusline_empty_payload_never_panics() {
        assert_eq!(render_statusline(&Value::Null), "claude");
        assert_eq!(render_statusline(&json!({})), "claude");
    }

    // --- env resolution precedence: socket vs HTTP fallback ---

    #[test]
    fn resolve_target_prefers_socket_when_set() {
        let target = resolve_target(
            Some("/run/asylum/asylum.sock".to_string()),
            Some("http://127.0.0.1:7717".to_string()),
            Some("tok".to_string()),
            PathBuf::from("/default/asylum.sock"),
        );
        assert_eq!(
            target,
            ClientTarget::Socket(PathBuf::from("/run/asylum/asylum.sock"))
        );
    }

    #[test]
    fn resolve_target_falls_back_to_http_with_token_when_no_socket() {
        let target = resolve_target(
            None,
            Some("http://127.0.0.1:9000".to_string()),
            Some("tok".to_string()),
            PathBuf::from("/default/asylum.sock"),
        );
        assert_eq!(
            target,
            ClientTarget::Http {
                base_url: "http://127.0.0.1:9000".to_string(),
                token: Some("tok".to_string()),
            }
        );
    }

    #[test]
    fn resolve_target_http_fallback_defaults_base_url_when_only_token_set() {
        let target = resolve_target(
            None,
            None,
            Some("tok".to_string()),
            PathBuf::from("/default/asylum.sock"),
        );
        assert_eq!(
            target,
            ClientTarget::Http {
                base_url: DEFAULT_BASE_URL.to_string(),
                token: Some("tok".to_string()),
            }
        );
    }

    #[test]
    fn resolve_target_defaults_to_local_socket_when_nothing_set() {
        let target = resolve_target(None, None, None, PathBuf::from("/default/asylum.sock"));
        assert_eq!(
            target,
            ClientTarget::Socket(PathBuf::from("/default/asylum.sock"))
        );
    }

    // --- exit-0-on-failure behavior (dispatch never panics; run_* always return ()) ---

    #[tokio::test]
    async fn dispatch_fails_gracefully_without_node_id() {
        let target = ClientTarget::Http {
            base_url: "http://127.0.0.1:1".to_string(),
            token: None,
        };
        let result = dispatch(None, target, Source::ClaudeHook, json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn dispatch_fails_gracefully_with_invalid_node_id() {
        let target = ClientTarget::Http {
            base_url: "http://127.0.0.1:1".to_string(),
            token: None,
        };
        let result = dispatch(
            Some("not-a-uuid".to_string()),
            target,
            Source::ClaudeHook,
            json!({}),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn dispatch_fails_gracefully_when_daemon_unreachable() {
        // Nothing listens on 127.0.0.1:1 (privileged/reserved); connection is
        // refused immediately rather than hanging for the 2s timeout.
        let target = ClientTarget::Http {
            base_url: "http://127.0.0.1:1".to_string(),
            token: None,
        };
        let result = dispatch(
            Some(Uuid::new_v4().to_string()),
            target,
            Source::ClaudeHook,
            json!({"hook_event_name": "Stop"}),
        )
        .await;
        assert!(result.is_err());
    }

    // --- full round trip against a real transport (no stub transports) ---

    async fn respond_once_tcp(listener: tokio::net::TcpListener, body: &'static [u8]) {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 8192];
        let _ = stream.read(&mut buf).await.expect("read request");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write response head");
        stream.write_all(body).await.expect("write response body");
        let _ = stream.shutdown().await;
    }

    #[tokio::test]
    async fn dispatch_round_trips_over_http_against_a_real_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("local_addr").port();
        let body: &'static [u8] =
            br#"{"accepted":true,"event":"node.turn_complete","session_id":"sess-http"}"#;
        let server = tokio::spawn(respond_once_tcp(listener, body));

        let target = ClientTarget::Http {
            base_url: format!("http://127.0.0.1:{port}"),
            token: Some("owner-token".to_string()),
        };
        let result = dispatch(
            Some(Uuid::new_v4().to_string()),
            target,
            Source::ClaudeHook,
            json!({"hook_event_name": "Stop"}),
        )
        .await
        .expect("dispatch should succeed against a real listening server");

        assert!(result.accepted);
        assert_eq!(result.session_id.as_deref(), Some("sess-http"));
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn dispatch_round_trips_over_unix_socket_against_a_real_server() {
        let dir = tempfile::tempdir().expect("tempdir");
        let socket_path = dir.path().join("bridge-test.sock");
        let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind unix socket");
        let body: &'static [u8] =
            br#"{"accepted":true,"event":"node.idle","session_id":"sess-sock"}"#;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf).await.expect("read request");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write response head");
            stream.write_all(body).await.expect("write response body");
            let _ = stream.shutdown().await;
        });

        let target = ClientTarget::Socket(socket_path);
        let result = dispatch(
            Some(Uuid::new_v4().to_string()),
            target,
            Source::ClaudeStatusline,
            json!({"context_window": {"used_percentage": 10.0}}),
        )
        .await
        .expect("dispatch should succeed against a real unix-socket server");

        assert!(result.accepted);
        assert_eq!(result.session_id.as_deref(), Some("sess-sock"));
        server.await.expect("server task");
    }
}
