//! Loon substrate: run an interactive Claude/Codex harness inside a real Loon
//! microVM with full Asylum integration.
//!
//! Architecture (see docs/superpowers/specs/2026-07-07-loon-guest-contract.md and
//! the C1 delivered contract):
//! - VM lifecycle + file staging use the `loon` CLI (`vm create`, `cp`, `vm
//!   stop|rm|prune`, non-PTY `exec` for mkdir). The CLI already resolves the
//!   client profile (url/key/pinned cert) from ~/.config/loon/config.toml.
//! - The interactive harness runs as a PTY exec driven over the loon daemon's
//!   HTTPS API directly (the CLI cannot allocate a PTY exec): `POST
//!   /instances/{id}/exec` with `pty:true` returns a stable exec id; a single
//!   long-lived attach WebSocket carries bidirectional PTY bytes; an SSE
//!   exec-stream is held in parallel purely as the authoritative exit signal.
//! - Output frames flow into the SAME sinks as the local substrate (transcript
//!   persistence + decision ingestion + exit truth), so observe/idle/exit work
//!   identically for Loon nodes.
//!
//! Asylum stays dumb plumbing: no harness output content is parsed in Rust; the
//! launch-prompt readiness gate and submit sequence are timing-only.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use asylum_types::api::SubstrateHealth;
use asylum_types::node::{CapabilitySnapshot, HarnessKind};
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::SubstrateContext;
use crate::decision_ingester::{
    DecisionProtocolRequest, StdoutDecisionIngestionEvent, StdoutDecisionLineIngestor,
};

/// Gap between the body burst and the submit carriage return, mirroring the
/// local substrate's submit contract (W0): long enough that the interactive TUI
/// processes them as two keystroke events, short enough to stay imperceptible.
const SUBMIT_GAP: Duration = Duration::from_millis(50);

/// Readiness gating for launch-prompt delivery over the guest PTY (timing only).
const LAUNCH_FIRST_OUTPUT_TIMEOUT: Duration = Duration::from_secs(45);
const LAUNCH_QUIET_WINDOW: Duration = Duration::from_millis(700);
const LAUNCH_READY_MAX: Duration = Duration::from_secs(20);

/// How long to wait for the freshly-created guest to answer a trivial exec
/// before giving up on provisioning (cold boot + guest agent up).
const GUEST_READY_TIMEOUT: Duration = Duration::from_secs(90);

/// In-guest absolute path the static musl `asylum` binary is staged to.
pub const GUEST_ASYLUM_BINARY: &str = "/usr/local/bin/asylum";

/// Health snapshot surfaced to the substrate-descriptor / health API. Fields are
/// consumed directly by `capability_service`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoonHealth {
    pub status: String,
    pub running_instances: Option<usize>,
    pub harness_profiles: Option<Vec<String>>,
}

/// Capability flags for a Loon node. The claude-dev image carries both claude
/// and codex on PATH, so when the loon host is reachable every interactive
/// capability is available. `resume` is intentionally false (Phase C2).
pub fn capability_flags_from_health(
    health: &LoonHealth,
    _harness: &HarnessKind,
) -> CapabilitySnapshot {
    let reachable = health.status == "ok";
    CapabilitySnapshot {
        browser_attach: true,
        native_attach: true,
        send_input: reachable,
        interrupt: reachable,
        stop: reachable,
        resume: false,
        structured_events: false,
        transcript_export: false,
    }
}

/// Everything the daemon threads into a Loon launch. Unlike the old thin
/// `LoonContext`, the full harness argv/env/workspace survive into the guest
/// launch (no lossy shim): `command`+`args` are the harness invocation (already
/// carrying the HTTP-resolved MCP/hook injection), `env` carries the guest-facing
/// ASYLUM_* resolution (base URL + per-node token + node id), `workspace` is the
/// in-guest working directory, and `launch_prompt` is delivered over the PTY as a
/// submitted message once the TUI is ready.
pub struct LoonLaunchSpec {
    pub node_id: Uuid,
    pub harness: HarnessKind,
    pub vm_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub workspace: Option<String>,
    pub launch_prompt: Option<String>,
}

impl LoonLaunchSpec {
    /// Convenience: derive from a `SubstrateContext` (the daemon builds one for
    /// symmetry with the local path) plus a VM name.
    pub fn from_context(ctx: SubstrateContext, vm_name: String) -> Self {
        Self {
            node_id: ctx.node_id,
            harness: ctx.harness,
            vm_name,
            command: ctx.command,
            args: ctx.args,
            env: ctx.env,
            workspace: ctx.workspace,
            launch_prompt: ctx.launch_prompt,
        }
    }
}

/// Static configuration for the Loon substrate, assembled from `AppConfig.loon`
/// plus the resolved guest-facing daemon URL.
#[derive(Clone, Debug)]
pub struct LoonRuntimeConfig {
    pub cli_path: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub profile: Option<String>,
    /// Override for the loon daemon API base (else taken from the client config).
    pub endpoint_override: Option<String>,
    pub image: String,
    pub workspace_dir: String,
    pub vm_memory_mib: u32,
    pub vm_cpus: u32,
    pub guest_asylum_binary: Option<PathBuf>,
    /// Base URL a guest uses to reach the Asylum daemon (injected as
    /// ASYLUM_BASE_URL on the guest harness).
    pub guest_base_url: String,
}

/// The loon client profile (url/key/pinned cert fingerprint) read from
/// ~/.config/loon/config.toml.
#[derive(Clone, Debug)]
struct LoonProfile {
    url: String,
    key: String,
    fingerprint_sha256: String,
}

#[derive(Clone)]
struct LoonRuntime {
    node_id: Uuid,
    vm_id: String,
    exec_id: String,
    input_tx: mpsc::UnboundedSender<Vec<u8>>,
    output_tx: broadcast::Sender<String>,
    tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

/// How a guest harness process ended (mirrors the local `ExitOutcome`).
type ExitSink = Arc<dyn Fn(Uuid, super::ExitOutcome) + Send + Sync>;
type OutputSink = Arc<dyn Fn(Uuid, &str) + Send + Sync>;
type DecisionSink = Arc<dyn Fn(Uuid, DecisionProtocolRequest) + Send + Sync>;

#[derive(Clone)]
pub struct LoonSubstrate {
    cfg: LoonRuntimeConfig,
    /// Resolved loon client profile + HTTP client, or None when the client
    /// config could not be read (health reports unavailable).
    profile: Option<LoonProfile>,
    http: Option<Client>,
    tls: Option<Arc<rustls::ClientConfig>>,
    runtimes: Arc<RwLock<HashMap<String, LoonRuntime>>>,
    output_sink: OutputSink,
    decision_sink: DecisionSink,
    exit_sink: ExitSink,
}

impl std::fmt::Debug for LoonSubstrate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoonSubstrate")
            .field("cfg", &self.cfg)
            .field("reachable", &self.http.is_some())
            .finish()
    }
}

impl LoonSubstrate {
    pub fn new<F, D, E>(
        cfg: LoonRuntimeConfig,
        output_sink: F,
        decision_sink: D,
        exit_sink: E,
    ) -> Self
    where
        F: Fn(Uuid, &str) + Send + Sync + 'static,
        D: Fn(Uuid, DecisionProtocolRequest) + Send + Sync + 'static,
        E: Fn(Uuid, super::ExitOutcome) + Send + Sync + 'static,
    {
        let profile = read_loon_profile(cfg.config_path.as_deref(), cfg.profile.as_deref())
            .map(|mut p| {
                if let Some(url) = &cfg.endpoint_override {
                    if !url.is_empty() {
                        p.url = url.clone();
                    }
                }
                p
            })
            .map_err(|e| {
                tracing::warn!(error = %e, "loon client config unreadable; loon substrate degraded");
                e
            })
            .ok();
        let tls = profile
            .as_ref()
            .and_then(|p| match build_pinned_tls(&p.fingerprint_sha256) {
                Ok(cfg) => Some(Arc::new(cfg)),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to build pinned TLS for loon");
                    None
                }
            });
        let http = tls.as_ref().and_then(|tls| {
            Client::builder()
                .use_preconfigured_tls((**tls).clone())
                .build()
                .map_err(|e| tracing::warn!(error = %e, "failed to build loon HTTP client"))
                .ok()
        });
        Self {
            cfg,
            profile,
            http,
            tls,
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            output_sink: Arc::new(output_sink),
            decision_sink: Arc::new(decision_sink),
            exit_sink: Arc::new(exit_sink),
        }
    }

    fn api_base(&self) -> Result<&str> {
        self.profile
            .as_ref()
            .map(|p| p.url.trim_end_matches('/'))
            .ok_or_else(|| anyhow!("loon client config not available"))
    }

    fn api_key(&self) -> Result<&str> {
        self.profile
            .as_ref()
            .map(|p| p.key.as_str())
            .ok_or_else(|| anyhow!("loon client config not available"))
    }

    fn http(&self) -> Result<&Client> {
        self.http
            .as_ref()
            .ok_or_else(|| anyhow!("loon HTTP client not available"))
    }

    // ---- health -----------------------------------------------------------

    pub async fn health(&self) -> Result<LoonHealth> {
        let base = match self.api_base() {
            Ok(b) => b.to_string(),
            Err(_) => {
                return Ok(LoonHealth {
                    status: "unavailable".to_string(),
                    running_instances: None,
                    harness_profiles: None,
                })
            }
        };
        let http = self.http()?;
        let key = self.api_key()?.to_string();
        // /instances is the authenticated liveness probe; count running rows.
        let resp = http
            .get(format!("{base}/instances"))
            .bearer_auth(&key)
            .timeout(Duration::from_secs(5))
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => {
                let running = r
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| count_running_instances(&v));
                Ok(LoonHealth {
                    status: "ok".to_string(),
                    running_instances: running,
                    harness_profiles: Some(vec!["claude_code".to_string(), "codex".to_string()]),
                })
            }
            _ => Ok(LoonHealth {
                status: "unavailable".to_string(),
                running_instances: None,
                harness_profiles: None,
            }),
        }
    }

    pub async fn health_api(&self) -> SubstrateHealth {
        match self.health().await {
            Ok(h) => SubstrateHealth {
                status: h.status,
                running_instances: h.running_instances,
                harness_profiles: h.harness_profiles,
            },
            Err(_) => SubstrateHealth {
                status: "unavailable".to_string(),
                running_instances: None,
                harness_profiles: None,
            },
        }
    }

    pub async fn check_support(&self, harness: &HarnessKind) -> Result<()> {
        let health = self.health().await?;
        if !capability_flags_from_health(&health, harness).send_input {
            return Err(anyhow!("unsupported_on_substrate"));
        }
        Ok(())
    }

    // ---- lifecycle --------------------------------------------------------

    pub async fn launch_node(&self, spec: LoonLaunchSpec) -> Result<String> {
        self.http()?; // fail fast if the loon host is unreachable
        let workspace = spec
            .workspace
            .clone()
            .unwrap_or_else(|| self.cfg.workspace_dir.clone());

        // 1. Create the VM from the claude-dev image.
        let vm_id = self.vm_create(&spec.vm_name).await?;

        // Any failure after create must tear the VM down so we never leak.
        let launched = self.provision_and_launch(&vm_id, &spec, &workspace).await;
        match launched {
            Ok(()) => Ok(vm_id),
            Err(e) => {
                tracing::warn!(error = %e, vm_id = %vm_id, "loon launch failed; tearing down VM");
                let _ = self.teardown_vm(&vm_id).await;
                Err(e)
            }
        }
    }

    async fn provision_and_launch(
        &self,
        vm_id: &str,
        spec: &LoonLaunchSpec,
        workspace: &str,
    ) -> Result<()> {
        // 2. Wait for the guest agent, then create working dirs.
        self.wait_guest_ready(vm_id).await?;
        self.guest_exec_oneshot(
            vm_id,
            &format!(
                "mkdir -p /root/.claude /root/.codex /usr/local/bin {}",
                shell_single_quote(workspace)
            ),
        )
        .await
        .context("create guest directories")?;

        // 3. Provision credentials + onboarding + the guest asylum binary.
        self.provision_files(vm_id, spec, workspace).await?;

        // 4. Start the interactive harness as a PTY exec over the HTTP API.
        let exec_id = self.exec_pty(vm_id, spec, workspace).await?;

        // 5. Wire the attach WebSocket (bidirectional PTY) + SSE exit watcher
        //    into the shared sinks, and deliver the launch prompt.
        self.spawn_streams(vm_id, &exec_id, spec).await?;
        Ok(())
    }

    async fn provision_files(
        &self,
        vm_id: &str,
        spec: &LoonLaunchSpec,
        workspace: &str,
    ) -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("cannot determine HOME"))?;

        // Claude subscription credentials (per the verified recipe).
        let claude_creds = home.join(".claude/.credentials.json");
        if claude_creds.exists() {
            self.guest_cp(vm_id, &claude_creds, "/root/.claude/.credentials.json", 384)
                .await
                .context("stage claude credentials")?;
        }
        // Codex credentials (harmless for a claude node; both harnesses share the image).
        let codex_creds = home.join(".codex/auth.json");
        if codex_creds.exists() {
            self.guest_cp(vm_id, &codex_creds, "/root/.codex/auth.json", 384)
                .await
                .context("stage codex credentials")?;
        }

        // Onboarding + workspace trust so the harness never enters a setup wizard.
        let claude_json = json!({
            "hasCompletedOnboarding": true,
            // Pre-accept the first-run Bypass Permissions mode warning so the
            // harness lands directly in an interactive prompt (it runs as root in
            // the microVM sandbox with --dangerously-skip-permissions).
            "bypassPermissionsModeAccepted": true,
            "projects": { workspace: { "hasTrustDialogAccepted": true } },
        });
        self.guest_write(vm_id, "/root/.claude.json", &claude_json.to_string(), 384)
            .await
            .context("write claude onboarding json")?;
        // Codex trust for the workspace.
        let codex_config = format!(
            "[projects.{}]\ntrust_level = \"trusted\"\n",
            toml_key(workspace)
        );
        self.guest_write(vm_id, "/root/.codex/config.toml", &codex_config, 384)
            .await
            .context("write codex trust config")?;

        // Stage the static musl asylum binary for the in-guest MCP + bridge.
        let binary = self.cfg.guest_asylum_binary.as_ref().ok_or_else(|| {
            anyhow!(
                "loon.guest_asylum_binary is not configured; build the static musl asylum \
                 binary (scripts/build-guest-asylum.sh) and set [loon] guest_asylum_binary"
            )
        })?;
        if !binary.exists() {
            return Err(anyhow!(
                "loon.guest_asylum_binary {} does not exist",
                binary.display()
            ));
        }
        self.stage_guest_binary(vm_id, binary, GUEST_ASYLUM_BINARY, 493)
            .await
            .context("stage guest asylum binary")?;
        let _ = spec;
        Ok(())
    }

    /// Stage a large binary into the guest. `loon cp` base64-encodes the whole
    /// file into a single JSON body bounded by the loon daemon's ~2 MiB request
    /// limit, so anything larger is split into sub-limit chunks, copied
    /// individually, and reassembled in-guest with `cat`.
    async fn stage_guest_binary(
        &self,
        vm_id: &str,
        src: &Path,
        dest: &str,
        mode: u32,
    ) -> Result<()> {
        // 1 MiB raw -> ~1.36 MiB base64, comfortably under loon's JSON body limit.
        const CHUNK: usize = 1024 * 1024;
        let data = tokio::fs::read(src).await.context("read guest binary")?;
        if data.len() <= CHUNK {
            return self.guest_cp(vm_id, src, dest, mode).await;
        }
        let stage_dir = format!("/tmp/asylum-stage-{}", Uuid::new_v4());
        self.guest_exec_oneshot(vm_id, &format!("mkdir -p {}", shell_single_quote(&stage_dir)))
            .await
            .context("create guest stage dir")?;
        for (idx, chunk) in data.chunks(CHUNK).enumerate() {
            let part = format!("{stage_dir}/part.{idx:04}");
            let mut tmp = std::env::temp_dir();
            tmp.push(format!("asylum-chunk-{}-{idx}", Uuid::new_v4()));
            tokio::fs::write(&tmp, chunk)
                .await
                .context("write chunk temp")?;
            let res = self.guest_cp(vm_id, &tmp, &part, 384).await;
            let _ = tokio::fs::remove_file(&tmp).await;
            res.with_context(|| format!("stage binary chunk {idx}"))?;
        }
        // Reassemble in lexical (== numeric, zero-padded) order, set mode, clean up.
        let assemble = format!(
            "cat {dir}/part.* > {dest} && chmod {mode:o} {dest} && rm -rf {dir}",
            dir = shell_single_quote(&stage_dir),
            dest = shell_single_quote(dest),
            mode = mode,
        );
        self.guest_exec_oneshot(vm_id, &assemble)
            .await
            .context("assemble guest binary")?;
        Ok(())
    }

    /// Start the harness as a PTY-backed exec and return the exec id. `sh -lc
    /// 'exec "$@"' _ <command> <args...>` passes argv through verbatim (no
    /// re-quoting) while giving a login PATH that includes the image's npm-global
    /// bin (claude/codex). HOME + the ASYLUM_* env are set on the exec directly.
    async fn exec_pty(&self, vm_id: &str, spec: &LoonLaunchSpec, workspace: &str) -> Result<String> {
        let base = self.api_base()?.to_string();
        let key = self.api_key()?.to_string();
        let http = self.http()?;

        let mut cmd = vec![
            "sh".to_string(),
            "-lc".to_string(),
            "exec \"$@\"".to_string(),
            "asylum-loon".to_string(),
            spec.command.clone(),
        ];
        cmd.extend(spec.args.iter().cloned());

        let mut env = serde_json::Map::new();
        env.insert("HOME".to_string(), json!("/root"));
        // The guest exec runs as root; a Loon microVM is a genuine sandbox, so
        // claude/codex are told so via IS_SANDBOX. Without it claude refuses
        // `--dangerously-skip-permissions` under root.
        env.insert("IS_SANDBOX".to_string(), json!("1"));
        for (k, v) in &spec.env {
            env.insert(k.clone(), json!(v));
        }

        let body = json!({
            "cmd": cmd,
            "env": env,
            "cwd": workspace,
            "pty": true,
        });
        let resp = http
            .post(format!("{base}/instances/{vm_id}/exec"))
            .bearer_auth(&key)
            .json(&body)
            .send()
            .await
            .context("start guest pty exec")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("guest pty exec failed: {status}: {text}"));
        }
        let value: serde_json::Value = resp.json().await.context("parse exec response")?;
        value
            .get("exec_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .ok_or_else(|| anyhow!("guest exec response missing exec_id: {value}"))
    }

    async fn spawn_streams(
        &self,
        vm_id: &str,
        exec_id: &str,
        spec: &LoonLaunchSpec,
    ) -> Result<()> {
        let base = self.api_base()?.to_string();
        let key = self.api_key()?.to_string();
        let tls = self
            .tls
            .clone()
            .ok_or_else(|| anyhow!("loon TLS config not available"))?;
        let http = self.http()?.clone();

        let node_id = spec.node_id;
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (output_tx, _) = broadcast::channel::<String>(1024);

        // --- attach WebSocket (bidirectional PTY bytes) ---
        let ws_url = format!(
            "{}/instances/{vm_id}/attach/{exec_id}",
            base.replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1)
        );
        let mut request = ws_url
            .clone()
            .into_client_request()
            .context("build attach ws request")?;
        request.headers_mut().insert(
            AUTHORIZATION,
            format!("Bearer {key}")
                .parse()
                .context("attach ws auth header")?,
        );
        let connector = Connector::Rustls(tls.clone());
        let (ws_stream, _resp) =
            connect_async_tls_with_config(request, None, false, Some(connector))
                .await
                .context("connect attach ws")?;
        let (mut ws_write, mut ws_read) = ws_stream.split();

        let output_sink = self.output_sink.clone();
        let decision_sink = self.decision_sink.clone();
        let output_tx_reader = output_tx.clone();
        let read_task = tokio::spawn(async move {
            let mut ingester = StdoutDecisionLineIngestor::default();
            while let Some(msg) = ws_read.next().await {
                let bytes = match msg {
                    Ok(Message::Binary(b)) => b.to_vec(),
                    Ok(Message::Text(t)) => t.as_bytes().to_vec(),
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => continue,
                };
                let chunk = String::from_utf8_lossy(&bytes).to_string();
                for event in ingester.ingest(&chunk) {
                    match event {
                        StdoutDecisionIngestionEvent::OutputText(text) => {
                            output_sink(node_id, &text);
                            let _ = output_tx_reader.send(text);
                        }
                        StdoutDecisionIngestionEvent::DecisionRequest(req) => {
                            decision_sink(node_id, req);
                        }
                    }
                }
                for event in ingester.flush_partial() {
                    if let StdoutDecisionIngestionEvent::OutputText(text) = event {
                        output_sink(node_id, &text);
                        let _ = output_tx_reader.send(text);
                    }
                }
            }
        });

        let write_task = tokio::spawn(async move {
            while let Some(bytes) = input_rx.recv().await {
                if ws_write.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
            let _ = ws_write.close().await;
        });

        // --- SSE exec-stream: authoritative exit signal ---
        let exit_sink = self.exit_sink.clone();
        let runtimes = self.runtimes.clone();
        let vm_id_owned = vm_id.to_string();
        let exec_id_owned = exec_id.to_string();
        let sse_url = format!("{base}/instances/{vm_id}/exec/{exec_id}");
        let sse_key = key.clone();
        let teardown = self.clone();
        let exit_task = tokio::spawn(async move {
            let outcome = watch_exit(&http, &sse_url, &sse_key).await;
            // The guest harness ended: drop the runtime, report exit truth, and
            // tear the VM down so a dead node never leaks a microVM.
            runtimes.write().await.remove(&vm_id_owned);
            exit_sink(node_id, outcome);
            let _ = teardown.teardown_vm(&vm_id_owned).await;
            let _ = exec_id_owned;
        });

        let runtime = LoonRuntime {
            node_id,
            vm_id: vm_id.to_string(),
            exec_id: exec_id.to_string(),
            input_tx: input_tx.clone(),
            output_tx: output_tx.clone(),
            tasks: Arc::new(Mutex::new(vec![read_task, write_task, exit_task])),
        };
        self.runtimes
            .write()
            .await
            .insert(vm_id.to_string(), runtime);

        // Resize the PTY off the hardcoded 80x24 attach default.
        let _ = self.exec_resize(vm_id, exec_id, 120, 40).await;

        // Deliver the launch prompt as a submitted message once the TUI settles.
        if let Some(prompt) = spec.launch_prompt.clone().filter(|p| !p.is_empty()) {
            let rx = output_tx.subscribe();
            tokio::spawn(await_ready_and_deliver(input_tx, rx, prompt, node_id));
        }
        Ok(())
    }

    // ---- input / control --------------------------------------------------

    pub async fn send_input(&self, external_id: &str, text: &str) -> Result<()> {
        let tx = self.input_for(external_id).await?;
        // W0 submit contract: body burst, gap, then a lone CR as a distinct write.
        tx.send(text.as_bytes().to_vec())
            .map_err(|_| anyhow!("node not running"))?;
        tokio::time::sleep(SUBMIT_GAP).await;
        tx.send(b"\r".to_vec())
            .map_err(|_| anyhow!("node not running"))?;
        Ok(())
    }

    /// Raw PTY write with no appended submit key (interactive attach path).
    pub async fn send_input_raw(&self, external_id: &str, bytes: &[u8]) -> Result<()> {
        let tx = self.input_for(external_id).await?;
        tx.send(bytes.to_vec())
            .map_err(|_| anyhow!("node not running"))?;
        Ok(())
    }

    pub async fn interrupt(&self, external_id: &str) -> Result<()> {
        // Ctrl-C as a PTY keystroke (ETX): the guest line discipline raises SIGINT
        // on the harness foreground process group, cancelling the turn without
        // killing the node (matches the local substrate + W1's interrupt fix).
        let tx = self.input_for(external_id).await?;
        tx.send(vec![0x03])
            .map_err(|_| anyhow!("node not running"))?;
        Ok(())
    }

    pub async fn stop(&self, external_id: &str) -> Result<()> {
        self.graceful_and_teardown(external_id).await
    }

    pub async fn archive(&self, external_id: &str) -> Result<()> {
        self.graceful_and_teardown(external_id).await
    }

    async fn graceful_and_teardown(&self, external_id: &str) -> Result<()> {
        let runtime = self.runtimes.write().await.remove(external_id);
        if let Some(runtime) = runtime {
            // Ask the harness to exit, then abort the stream tasks.
            let _ = self.exec_signal(&runtime.vm_id, &runtime.exec_id, "SIGTERM").await;
            let tasks = std::mem::take(&mut *runtime.tasks.lock().await);
            for t in tasks {
                t.abort();
            }
        }
        self.teardown_vm(external_id).await
    }

    async fn teardown_vm(&self, vm_id: &str) -> Result<()> {
        let _ = self.run_cli(&["vm", "stop", vm_id]).await;
        let _ = self.run_cli(&["vm", "rm", vm_id]).await;
        // Prune the destroyed tombstone (and dependent rows) after teardown.
        let _ = self.run_cli(&["vm", "prune"]).await;
        Ok(())
    }

    pub async fn attach(&self, external_id: &str) -> Result<broadcast::Receiver<String>> {
        let runtimes = self.runtimes.read().await;
        let runtime = runtimes
            .get(external_id)
            .ok_or_else(|| anyhow!("node not running"))?;
        Ok(runtime.output_tx.subscribe())
    }

    pub async fn has_runtime(&self, external_id: &str) -> bool {
        self.runtimes.read().await.contains_key(external_id)
    }

    /// Whether a VM with this instance id still exists on the loon host. Queries
    /// the authenticated `/instances` listing (destroyed-hidden by default, so a
    /// torn-down VM does not appear) and checks membership. Used by startup
    /// reconciliation to distinguish a VM that outlived the daemon from one that
    /// is already gone. Errors (host unreachable) propagate so the caller can be
    /// conservative rather than silently declaring a VM dead.
    pub async fn vm_exists(&self, external_id: &str) -> Result<bool> {
        let base = self.api_base()?.to_string();
        let key = self.api_key()?.to_string();
        let http = self.http()?;
        let resp = http
            .get(format!("{base}/instances"))
            .bearer_auth(&key)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .context("list loon instances")?;
        if !resp.status().is_success() {
            return Err(anyhow!("loon /instances returned {}", resp.status()));
        }
        let value = resp
            .json::<serde_json::Value>()
            .await
            .context("parse loon /instances body")?;
        Ok(instance_ids(&value).iter().any(|id| id == external_id))
    }

    /// Tear a VM down by instance id (stop + rm + prune), regardless of whether an
    /// in-memory runtime exists. Used by startup reconciliation to reclaim VMs
    /// orphaned by a daemon restart (the guest workspace does not survive, so the
    /// node is not resumable and the honest action is teardown).
    pub async fn force_teardown(&self, external_id: &str) -> Result<()> {
        self.teardown_vm(external_id).await
    }

    pub async fn list_nodes(&self) -> Vec<Uuid> {
        self.runtimes
            .read()
            .await
            .values()
            .map(|r| r.node_id)
            .collect()
    }

    async fn input_for(&self, external_id: &str) -> Result<mpsc::UnboundedSender<Vec<u8>>> {
        let runtimes = self.runtimes.read().await;
        let runtime = runtimes
            .get(external_id)
            .ok_or_else(|| anyhow!("node not running"))?;
        Ok(runtime.input_tx.clone())
    }

    // ---- loon HTTP helpers ------------------------------------------------

    async fn exec_signal(&self, vm_id: &str, exec_id: &str, signal: &str) -> Result<()> {
        let base = self.api_base()?.to_string();
        let key = self.api_key()?.to_string();
        let http = self.http()?;
        http.post(format!("{base}/instances/{vm_id}/exec/{exec_id}/signal"))
            .bearer_auth(&key)
            .json(&json!({ "signal": signal }))
            .send()
            .await
            .context("send exec signal")?;
        Ok(())
    }

    async fn exec_resize(&self, vm_id: &str, exec_id: &str, cols: u32, rows: u32) -> Result<()> {
        let base = self.api_base()?.to_string();
        let key = self.api_key()?.to_string();
        let http = self.http()?;
        http.post(format!("{base}/instances/{vm_id}/exec/{exec_id}/resize"))
            .bearer_auth(&key)
            .json(&json!({ "cols": cols, "rows": rows }))
            .send()
            .await
            .context("resize exec pty")?;
        Ok(())
    }

    // ---- loon CLI helpers (VM lifecycle + file staging) -------------------

    async fn vm_create(&self, name: &str) -> Result<String> {
        let memory = self.cfg.vm_memory_mib.to_string();
        let cpus = self.cfg.vm_cpus.to_string();
        let out = self
            .run_cli(&[
                "--json", "vm", "create", &self.cfg.image, "--name", name, "--memory", &memory,
                "--cpus", &cpus,
            ])
            .await
            .context("loon vm create")?;
        parse_instance_id(&out).ok_or_else(|| anyhow!("loon vm create did not return an id: {out}"))
    }

    /// Poll a trivial non-PTY exec until the guest agent answers (cold boot).
    async fn wait_guest_ready(&self, vm_id: &str) -> Result<()> {
        let deadline = tokio::time::Instant::now() + GUEST_READY_TIMEOUT;
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            if self
                .guest_exec_oneshot(vm_id, "true")
                .await
                .is_ok()
            {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(anyhow!("guest {vm_id} not ready after {GUEST_READY_TIMEOUT:?}"));
            }
            tokio::time::sleep(Duration::from_millis(if attempt < 5 { 500 } else { 2000 })).await;
        }
    }

    /// Run a one-shot command in the guest via the CLI's non-PTY exec path.
    async fn guest_exec_oneshot(&self, vm_id: &str, script: &str) -> Result<()> {
        self.run_cli(&["exec", vm_id, "--", "sh", "-lc", script])
            .await
            .map(|_| ())
    }

    async fn guest_cp(
        &self,
        vm_id: &str,
        src: &Path,
        dest: &str,
        mode: u32,
    ) -> Result<()> {
        let dest_arg = format!("{vm_id}:{dest}");
        let mode_s = mode.to_string();
        self.run_cli(&[
            "cp",
            &src.to_string_lossy(),
            &dest_arg,
            "--mode",
            &mode_s,
        ])
        .await
        .map(|_| ())
    }

    /// Stage inline content to a guest path via a temp file + cp.
    async fn guest_write(&self, vm_id: &str, dest: &str, content: &str, mode: u32) -> Result<()> {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("asylum-loon-{}", Uuid::new_v4()));
        tokio::fs::write(&tmp, content)
            .await
            .context("write temp provision file")?;
        let res = self.guest_cp(vm_id, &tmp, dest, mode).await;
        let _ = tokio::fs::remove_file(&tmp).await;
        res
    }

    fn cli_binary(&self) -> PathBuf {
        self.cfg
            .cli_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("loon"))
    }

    async fn run_cli(&self, args: &[&str]) -> Result<String> {
        let mut command = Command::new(self.cli_binary());
        // Global options (config/profile) precede the subcommand.
        if let Some(config_path) = &self.cfg.config_path {
            command.arg("--config").arg(config_path);
        }
        if let Some(profile) = &self.cfg.profile {
            command.arg("--profile").arg(profile);
        }
        command.args(args);
        let output = command.output().await.context("spawn loon CLI")?;
        if !output.status.success() {
            return Err(anyhow!(
                "loon {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Hold the SSE exec-stream open and map its `exit` event to an `ExitOutcome`.
/// For a PTY exec this stream carries only the exit notification.
async fn watch_exit(http: &Client, url: &str, key: &str) -> super::ExitOutcome {
    let resp = match http.get(url).bearer_auth(key).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
            return super::ExitOutcome {
                success: false,
                code: None,
            }
        }
    };
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(_) => break,
        };
        buf.push_str(&String::from_utf8_lossy(&bytes));
        // Parse complete SSE data lines looking for an exit_code.
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            if let Some(data) = line.strip_prefix("data:") {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(data.trim()) {
                    if let Some(code) = value.get("exit_code").and_then(|v| v.as_i64()) {
                        return super::ExitOutcome {
                            success: code == 0,
                            code: Some(code as u32),
                        };
                    }
                }
            }
        }
    }
    // Stream ended without a parseable exit code: treat as a clean end.
    super::ExitOutcome {
        success: true,
        code: None,
    }
}

/// Wait for the guest TUI to settle (first frame + a quiet window, all bounded),
/// then deliver the launch prompt as a submitted message over the PTY input
/// channel. Timing only; no output content is inspected.
async fn await_ready_and_deliver(
    input_tx: mpsc::UnboundedSender<Vec<u8>>,
    mut rx: broadcast::Receiver<String>,
    prompt: String,
    node_id: Uuid,
) {
    let _ = tokio::time::timeout(LAUNCH_FIRST_OUTPUT_TIMEOUT, rx.recv()).await;
    let deadline = tokio::time::Instant::now() + LAUNCH_READY_MAX;
    while let Ok(Ok(_)) = tokio::time::timeout(LAUNCH_QUIET_WINDOW, rx.recv()).await {
        // Output still arriving inside the window: keep waiting unless the overall
        // readiness deadline has passed.
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }
    if input_tx.send(prompt.into_bytes()).is_err() {
        tracing::warn!(node_id = %node_id, "loon launch prompt delivery failed (body)");
        return;
    }
    tokio::time::sleep(SUBMIT_GAP).await;
    if input_tx.send(b"\r".to_vec()).is_err() {
        tracing::warn!(node_id = %node_id, "loon launch prompt delivery failed (submit)");
        return;
    }
    tracing::debug!(node_id = %node_id, "delivered loon launch prompt");
}

// ---- loon client config + TLS pinning -------------------------------------

fn loon_config_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("loon/config.toml"));
        }
    }
    dirs::home_dir().map(|h| h.join(".config/loon/config.toml"))
}

fn read_loon_profile(explicit: Option<&Path>, profile: Option<&str>) -> Result<LoonProfile> {
    let path = loon_config_path(explicit)
        .ok_or_else(|| anyhow!("cannot locate loon client config"))?;
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read loon config {}", path.display()))?;
    let doc: toml::Value = raw.parse().context("parse loon config")?;
    let default_profile = doc
        .get("default_profile")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let name = profile.unwrap_or(default_profile);
    let profiles = doc
        .get("profiles")
        .and_then(|v| v.as_table())
        .ok_or_else(|| anyhow!("loon config has no [profiles]"))?;
    let p = profiles
        .get(name)
        .and_then(|v| v.as_table())
        .ok_or_else(|| anyhow!("loon config profile '{name}' not found"))?;
    let url = p
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("loon profile '{name}' missing url"))?
        .to_string();
    let key = p
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("loon profile '{name}' missing key"))?
        .to_string();
    let fingerprint_sha256 = p
        .get("fingerprint_sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("loon profile '{name}' missing fingerprint_sha256"))?
        .to_string();
    Ok(LoonProfile {
        url,
        key,
        fingerprint_sha256,
    })
}

/// Build a rustls client config that pins the loon daemon's self-signed leaf by
/// its SHA-256 fingerprint (the same trust anchor the loon CLI uses). Shared by
/// the reqwest control client and the attach WebSocket.
fn build_pinned_tls(fingerprint_hex: &str) -> Result<rustls::ClientConfig> {
    let want = decode_hex(fingerprint_hex)
        .ok_or_else(|| anyhow!("invalid loon cert fingerprint"))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(PinnedCertVerifier {
        fingerprint: want,
        provider: provider.clone(),
    });
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("rustls protocol versions")?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok(config)
}

#[derive(Debug)]
struct PinnedCertVerifier {
    fingerprint: Vec<u8>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl rustls::client::danger::ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls_pki_types::CertificateDer<'_>,
        _intermediates: &[rustls_pki_types::CertificateDer<'_>],
        _server_name: &rustls_pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        use sha2::{Digest, Sha256};
        let got = Sha256::digest(end_entity.as_ref());
        if got.as_slice() == self.fingerprint.as_slice() {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "loon cert fingerprint mismatch".to_string(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls_pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls_pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async_tls_with_config, Connector};

// ---- small helpers --------------------------------------------------------

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim().replace(':', "");
    if s.len() & 1 == 1 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn count_running_instances(value: &serde_json::Value) -> Option<usize> {
    let arr = value
        .as_array()
        .or_else(|| value.get("instances").and_then(|v| v.as_array()))?;
    Some(
        arr.iter()
            .filter(|inst| {
                inst.get("state")
                    .or_else(|| inst.get("status"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.eq_ignore_ascii_case("running"))
                    .unwrap_or(false)
            })
            .count(),
    )
}

/// Extract every instance id from a `/instances` listing body. Tolerant of the
/// exact JSON shape (bare array or `{"instances": [...]}`) and of the id field
/// name (`id`/`instance_id`/`vm_id`/`instanceId`), mirroring `parse_instance_id`.
fn instance_ids(value: &serde_json::Value) -> Vec<String> {
    let arr = value
        .as_array()
        .or_else(|| value.get("instances").and_then(|v| v.as_array()));
    let mut out = Vec::new();
    if let Some(arr) = arr {
        for inst in arr {
            for key in ["id", "instance_id", "vm_id", "instanceId"] {
                if let Some(id) = inst.get(key).and_then(|v| v.as_str()) {
                    out.push(id.to_string());
                    break;
                }
            }
        }
    }
    out
}

/// Parse the instance id from `loon --json vm create` output. Tolerant of the
/// exact JSON shape: looks for an `id`/`instance_id` field, else the first UUID.
fn parse_instance_id(output: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output.trim()) {
        for key in ["id", "instance_id", "vm_id", "instanceId"] {
            if let Some(id) = value.get(key).and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
        if let Some(inst) = value.get("instance") {
            for key in ["id", "instance_id"] {
                if let Some(id) = inst.get(key).and_then(|v| v.as_str()) {
                    return Some(id.to_string());
                }
            }
        }
    }
    // Fallback: first UUID-looking token anywhere in the output.
    output
        .split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .find(|tok| Uuid::parse_str(tok).is_ok())
        .map(ToString::to_string)
}

fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn toml_key(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_hex_fingerprint() {
        assert_eq!(decode_hex("00ff10"), Some(vec![0x00, 0xff, 0x10]));
        assert_eq!(decode_hex("aa:bb"), Some(vec![0xaa, 0xbb]));
        assert_eq!(decode_hex("abc"), None);
    }

    #[test]
    fn parses_instance_id_from_json_and_text() {
        let id = "019f3a5a-f01c-7dc2-bc67-ffd09816eb23";
        assert_eq!(
            parse_instance_id(&format!("{{\"id\":\"{id}\"}}")),
            Some(id.to_string())
        );
        assert_eq!(
            parse_instance_id(&format!("{{\"instance\":{{\"id\":\"{id}\"}}}}")),
            Some(id.to_string())
        );
        assert_eq!(
            parse_instance_id(&format!("created {id} running\n")),
            Some(id.to_string())
        );
    }

    #[test]
    fn counts_running_instances_across_shapes() {
        let v = serde_json::json!([
            {"state": "running"},
            {"state": "stopped"},
            {"status": "Running"}
        ]);
        assert_eq!(count_running_instances(&v), Some(2));
        let wrapped = serde_json::json!({"instances": [{"state":"running"}]});
        assert_eq!(count_running_instances(&wrapped), Some(1));
    }

    #[test]
    fn capability_flags_track_reachability() {
        let ok = LoonHealth {
            status: "ok".to_string(),
            running_instances: Some(0),
            harness_profiles: None,
        };
        assert!(capability_flags_from_health(&ok, &HarnessKind::ClaudeCode).send_input);
        let down = LoonHealth {
            status: "unavailable".to_string(),
            running_instances: None,
            harness_profiles: None,
        };
        assert!(!capability_flags_from_health(&down, &HarnessKind::Codex).send_input);
    }

    #[test]
    fn launch_spec_carries_full_context() {
        let ctx = SubstrateContext {
            node_id: Uuid::new_v4(),
            harness: HarnessKind::ClaudeCode,
            command: "claude".to_string(),
            args: vec!["--mcp-config".to_string(), "{}".to_string()],
            workspace: Some("/work".to_string()),
            env: vec![("ASYLUM_TOKEN".to_string(), "t".to_string())],
            launch_prompt: Some("hi".to_string()),
        };
        let spec = LoonLaunchSpec::from_context(ctx, "asylum-x".to_string());
        assert_eq!(spec.command, "claude");
        assert_eq!(spec.args.len(), 2);
        assert_eq!(spec.env[0].0, "ASYLUM_TOKEN");
        assert_eq!(spec.workspace.as_deref(), Some("/work"));
    }
}
