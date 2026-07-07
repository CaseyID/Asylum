use anyhow::{anyhow, Context, Result};
use asylum_types::node::{CapabilitySnapshot, HarnessKind};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;

/// Gap between the body burst and the submit carriage return in `send_input`.
/// Long enough that the interactive TUI processes them as two separate keystroke
/// events (so the CR is not absorbed into a paste), short enough to stay
/// imperceptible.
const SUBMIT_GAP: Duration = Duration::from_millis(50);

/// Readiness gating for launch-prompt delivery. Wait for the harness to emit its
/// first PTY frame, then for its output to go quiet (the initial render
/// settling) before typing the prompt, so it lands in a ready input box rather
/// than being eaten by a redraw. All bounded so a silent or chatty harness still
/// gets its prompt.
const LAUNCH_FIRST_OUTPUT_TIMEOUT: Duration = Duration::from_secs(20);
const LAUNCH_QUIET_WINDOW: Duration = Duration::from_millis(600);
const LAUNCH_READY_MAX: Duration = Duration::from_secs(10);

/// Deliver `text` to a writer as a SUBMITTED message: write the body, pause, then
/// write a lone carriage return as a DISTINCT write. Claude's TUI uses auto-paste
/// / bracketed-paste detection, so a CR bundled into the same write as the body
/// is absorbed as pasted content and never submits; only a CR arriving as its own
/// keystroke submits. codex submits on the lone CR the same way, so this sequence
/// is correct for both harnesses.
async fn submit_over_writer(writer: &Arc<Mutex<Box<dyn Write + Send>>>, text: &str) -> Result<()> {
    {
        let mut w = writer.lock().await;
        w.write_all(text.as_bytes())?;
        w.flush()?;
    }
    tokio::time::sleep(SUBMIT_GAP).await;
    {
        let mut w = writer.lock().await;
        w.write_all(b"\r")?;
        w.flush()?;
    }
    Ok(())
}

/// Wait for the harness TUI to be ready, then deliver its launch prompt as a
/// submitted message. Ready = first PTY frame seen, then output quiet for a full
/// window (initial render finished). No output content is inspected — this is
/// timing only, keeping Asylum as dumb plumbing.
async fn await_ready_and_deliver(
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    mut rx: broadcast::Receiver<String>,
    prompt: String,
    node_id: Uuid,
) {
    // First frame (bounded: deliver anyway if the harness never prints).
    let _ = tokio::time::timeout(LAUNCH_FIRST_OUTPUT_TIMEOUT, rx.recv()).await;
    // Then wait for the render to settle: no new output for a full quiet window,
    // bounded by LAUNCH_READY_MAX so a continuously-redrawing TUI still proceeds.
    let deadline = tokio::time::Instant::now() + LAUNCH_READY_MAX;
    loop {
        match tokio::time::timeout(LAUNCH_QUIET_WINDOW, rx.recv()).await {
            // Output arrived inside the window: still rendering; keep waiting
            // unless we have hit the overall deadline.
            Ok(Ok(_)) => {
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
            }
            // Quiet window elapsed, or the channel closed/lagged: treat as ready.
            _ => break,
        }
    }
    match submit_over_writer(&writer, &prompt).await {
        Ok(()) => tracing::debug!(node_id = %node_id, "delivered launch prompt"),
        Err(e) => {
            tracing::warn!(node_id = %node_id, error = %e, "launch prompt delivery failed")
        }
    }
}

use super::SubstrateContext;
use crate::decision_ingester::{
    DecisionProtocolRequest, StdoutDecisionIngestionEvent, StdoutDecisionLineIngestor,
};

#[derive(Clone)]
struct LocalRuntime {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    output_tx: broadcast::Sender<String>,
    killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
}

/// How a local harness process ended, reported to the exit sink so the
/// capability service can distinguish a clean exit (`node.exited`) from an
/// abnormal one (`node.errored`).
#[derive(Clone, Copy, Debug)]
pub struct ExitOutcome {
    pub success: bool,
    pub code: Option<u32>,
}

#[derive(Clone)]
pub struct LocalSubstrate {
    runtimes: Arc<RwLock<HashMap<Uuid, LocalRuntime>>>,
    output_sink: Arc<dyn Fn(Uuid, &str) + Send + Sync>,
    decision_sink: Arc<dyn Fn(Uuid, DecisionProtocolRequest) + Send + Sync>,
    exit_sink: Arc<dyn Fn(Uuid, ExitOutcome) + Send + Sync>,
}

impl LocalSubstrate {
    pub fn new<F>(output_sink: F) -> Self
    where
        F: Fn(Uuid, &str) + Send + Sync + 'static,
    {
        Self {
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            output_sink: Arc::new(output_sink),
            decision_sink: Arc::new(|_, _| {}),
            exit_sink: Arc::new(|_, _| {}),
        }
    }

    pub fn new_with_decision_sink<F, D>(output_sink: F, decision_sink: D) -> Self
    where
        F: Fn(Uuid, &str) + Send + Sync + 'static,
        D: Fn(Uuid, DecisionProtocolRequest) + Send + Sync + 'static,
    {
        Self {
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            output_sink: Arc::new(output_sink),
            decision_sink: Arc::new(decision_sink),
            exit_sink: Arc::new(|_, _| {}),
        }
    }

    pub fn new_with_sinks<F, D, E>(output_sink: F, decision_sink: D, exit_sink: E) -> Self
    where
        F: Fn(Uuid, &str) + Send + Sync + 'static,
        D: Fn(Uuid, DecisionProtocolRequest) + Send + Sync + 'static,
        E: Fn(Uuid, ExitOutcome) + Send + Sync + 'static,
    {
        Self {
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            output_sink: Arc::new(output_sink),
            decision_sink: Arc::new(decision_sink),
            exit_sink: Arc::new(exit_sink),
        }
    }

    pub async fn has_runtime(&self, node_id: Uuid) -> bool {
        self.runtimes.read().await.contains_key(&node_id)
    }

    pub async fn launch(&self, ctx: SubstrateContext) -> Result<()> {
        let pty = native_pty_system().openpty(PtySize::default())?;
        let command = {
            let mut builder = CommandBuilder::new(ctx.command);
            for arg in ctx.args {
                builder.arg(arg);
            }
            if let Some(workspace) = ctx.workspace {
                builder.cwd(workspace);
            }
            for (key, value) in ctx.env {
                builder.env(key, value);
            }
            builder
        };

        let child = pty
            .slave
            .spawn_command(command)
            .context("spawn local harness process")?;
        let killer = child.clone_killer();
        let mut reader = pty.master.try_clone_reader()?;
        let writer = pty.master.take_writer()?;

        let node_id = ctx.node_id;
        let sink = self.output_sink.clone();
        let decision_sink = self.decision_sink.clone();
        let exit_sink = self.exit_sink.clone();
        let runtimes_for_exit = self.runtimes.clone();
        let writer_arc: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(writer));
        let (output_tx, _) = broadcast::channel(1024);
        let output_tx_for_reader = output_tx.clone();

        // If a launch prompt is set, capture a writer handle and an output
        // subscription now — before `output_tx`/`writer_arc` are moved into the
        // runtime — so a background task can deliver the prompt as a submitted
        // message once the TUI is ready (bug fix: a positional prompt argv is
        // never auto-submitted by interactive harnesses).
        let launch_delivery = ctx.launch_prompt.clone().filter(|p| !p.is_empty()).map(
            |prompt| (prompt, output_tx.subscribe(), writer_arc.clone()),
        );

        // Insert into runtimes before spawning the reader task so that any
        // output emitted immediately on startup is not dropped (L3).
        self.runtimes.write().await.insert(
            node_id,
            LocalRuntime {
                writer: writer_arc,
                output_tx,
                killer: Arc::new(Mutex::new(killer)),
            },
        );

        if let Some((prompt, rx, writer)) = launch_delivery {
            tokio::spawn(await_ready_and_deliver(writer, rx, prompt, node_id));
        }

        tokio::task::spawn_blocking(move || {
            let mut local_child = child;
            let mut ingester = StdoutDecisionLineIngestor::default();
            let mut buffer = [0_u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        let chunk = String::from_utf8_lossy(&buffer[..size]).to_string();
                        let line_events = ingester.ingest(&chunk);
                        let partial_events = ingester.flush_partial();
                        for event in line_events.into_iter().chain(partial_events) {
                            match event {
                                StdoutDecisionIngestionEvent::OutputText(text) => {
                                    sink(node_id, &text);
                                    let _ = output_tx_for_reader.send(text);
                                }
                                StdoutDecisionIngestionEvent::DecisionRequest(request) => {
                                    (decision_sink)(node_id, request);
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            for event in ingester.finalize() {
                match event {
                    StdoutDecisionIngestionEvent::OutputText(text) => {
                        sink(node_id, &text);
                        let _ = output_tx_for_reader.send(text);
                    }
                    StdoutDecisionIngestionEvent::DecisionRequest(request) => {
                        (decision_sink)(node_id, request);
                    }
                }
            }
            let outcome = match local_child.wait() {
                Ok(status) => ExitOutcome {
                    success: status.success(),
                    code: Some(status.exit_code()),
                },
                Err(_) => ExitOutcome {
                    success: false,
                    code: None,
                },
            };
            // Harness exited (either on its own or via stop()); drop the runtime
            // so the daemon's view stays consistent and notify the capability
            // service so it can transition liveness.
            let runtimes_clone = runtimes_for_exit.clone();
            tokio::runtime::Handle::current().spawn(async move {
                runtimes_clone.write().await.remove(&node_id);
            });
            (exit_sink)(node_id, outcome);
        });

        Ok(())
    }

    /// Deliver `text` as a SUBMITTED message to the node's interactive TUI: the
    /// body and the submit key are written as two distinct PTY writes (see
    /// `submit_over_writer`). This is the path a supervisor, a hook action, or
    /// decision feedback uses to drive a worker, so a single call must both enter
    /// the text AND submit it.
    pub async fn send_input(&self, node_id: Uuid, text: &str) -> Result<()> {
        let writer = self.writer_for(node_id).await?;
        submit_over_writer(&writer, text).await
    }

    /// Write bytes to the node's PTY verbatim, with NO appended submit key. This
    /// is the raw terminal path used by interactive attach (browser/native),
    /// where the caller's own keystrokes — including Enter — are already in the
    /// byte stream. Appending a CR here would double-submit attached terminals.
    pub async fn send_input_raw(&self, node_id: Uuid, text: &str) -> Result<()> {
        let writer = self.writer_for(node_id).await?;
        let mut writer = writer.lock().await;
        writer.write_all(text.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    /// Clone the shared writer handle for a running node, releasing the runtimes
    /// map lock before the caller writes (so a slow PTY write never blocks other
    /// runtime lookups).
    async fn writer_for(&self, node_id: Uuid) -> Result<Arc<Mutex<Box<dyn Write + Send>>>> {
        let runtimes = self.runtimes.read().await;
        let runtime = runtimes
            .get(&node_id)
            .ok_or_else(|| anyhow!("node not running"))?;
        Ok(runtime.writer.clone())
    }

    pub async fn attach(&self, node_id: Uuid) -> Result<broadcast::Receiver<String>> {
        let runtimes = self.runtimes.read().await;
        let runtime = runtimes
            .get(&node_id)
            .ok_or_else(|| anyhow!("node not running"))?;
        Ok(runtime.output_tx.subscribe())
    }

    pub async fn interrupt(&self, node_id: Uuid) -> Result<()> {
        let runtimes = self.runtimes.read().await;
        let runtime = runtimes
            .get(&node_id)
            .ok_or_else(|| anyhow!("node not running"))?;
        let mut writer = runtime.writer.lock().await;
        writer.write_all(&[0x03])?;
        writer.flush()?;
        Ok(())
    }

    pub async fn stop(&self, node_id: Uuid) -> Result<()> {
        let runtime = {
            let mut runtimes = self.runtimes.write().await;
            runtimes.remove(&node_id)
        };
        if let Some(runtime) = runtime {
            let mut killer = runtime.killer.lock().await;
            let _ = killer.kill();
        }
        Ok(())
    }

    pub async fn capabilities(&self, harness: HarnessKind) -> CapabilitySnapshot {
        let _ = harness;
        CapabilitySnapshot {
            browser_attach: true,
            native_attach: true,
            send_input: true,
            interrupt: true,
            stop: true,
            resume: false,
            structured_events: false,
            transcript_export: false,
        }
    }

    pub async fn list_nodes(&self) -> Vec<Uuid> {
        self.runtimes.read().await.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// A `Write` that records each `write_all` call as a distinct entry, so tests
    /// can assert the submit sequence really performs two separate writes rather
    /// than one bundled `text + "\r"` (the bracketed-paste bug).
    #[derive(Clone, Default)]
    struct RecordingWriter {
        writes: Arc<StdMutex<Vec<Vec<u8>>>>,
    }

    impl Write for RecordingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.writes.lock().unwrap().push(buf.to_vec());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn submit_sequence_writes_body_then_a_distinct_carriage_return() {
        let recorder = RecordingWriter::default();
        let writes = recorder.writes.clone();
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(recorder)));

        submit_over_writer(&writer, "Reply with exactly: SEND-OK")
            .await
            .expect("submit sequence should succeed");

        let recorded = writes.lock().unwrap();
        assert_eq!(
            recorded.len(),
            2,
            "submit must be two writes (body, then CR), got {recorded:?}"
        );
        assert_eq!(recorded[0], b"Reply with exactly: SEND-OK");
        assert_eq!(
            recorded[1], b"\r",
            "the carriage return must be its own write so the TUI submits it"
        );
    }

    #[tokio::test]
    async fn submit_sequence_never_bundles_cr_into_the_body_write() {
        // Regression guard for the original bug: `text + "\r"` in a single write
        // is absorbed as pasted content by claude's TUI and never submits.
        let recorder = RecordingWriter::default();
        let writes = recorder.writes.clone();
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(recorder)));

        submit_over_writer(&writer, "hello").await.unwrap();

        let recorded = writes.lock().unwrap();
        assert!(
            !recorded[0].contains(&b'\r'),
            "body write must not contain a carriage return"
        );
    }

    /// End-to-end launch delivery through a real PTY. A harness that prints a
    /// readiness frame then idles is launched with a `launch_prompt`. The PTY
    /// line discipline echoes delivered input back to the master, which the
    /// output sink captures — proving the prompt is typed into the ready TUI and
    /// submitted, without any argv positional.
    #[tokio::test]
    async fn launch_delivers_prompt_over_pty_after_readiness() {
        let collected: Arc<StdMutex<String>> = Arc::new(StdMutex::new(String::new()));
        let sink = collected.clone();
        let substrate = LocalSubstrate::new(move |_id, text| {
            sink.lock().unwrap().push_str(text);
        });

        let node_id = Uuid::new_v4();
        let ctx = crate::substrate::SubstrateContext {
            node_id,
            harness: HarnessKind::Codex,
            command: "sh".to_string(),
            // Emit a readiness frame (so the delivery task's readiness gate
            // fires), then idle so the PTY stays open and echoes the delivery.
            args: vec!["-c".to_string(), "printf 'READY'; sleep 30".to_string()],
            workspace: None,
            env: Vec::new(),
            launch_prompt: Some("ASYLUM-LAUNCH-MARKER".to_string()),
        };
        substrate.launch(ctx).await.expect("launch should succeed");

        let mut delivered = false;
        for _ in 0..240 {
            if collected.lock().unwrap().contains("ASYLUM-LAUNCH-MARKER") {
                delivered = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let _ = substrate.stop(node_id).await;
        assert!(
            delivered,
            "launch prompt was not delivered/submitted over the PTY; captured: {:?}",
            collected.lock().unwrap()
        );
    }
}
