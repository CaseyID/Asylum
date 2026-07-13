use anyhow::{anyhow, Context, Result};
use asylum_types::node::{CapabilitySnapshot, HarnessKind};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex, Notify, RwLock};
use uuid::Uuid;

/// Gap between the body burst and the submit carriage return in `send_input`.
/// Long enough that the interactive TUI processes them as two separate keystroke
/// events (so the CR is not absorbed into a paste), short enough to stay
/// imperceptible.
// Gap between the typed body and the submitting CR. Must clear codex's
// paste-burst Enter suppression: after a paste-like burst, codex treats an
// Enter arriving within PASTE_ENTER_SUPPRESS_WINDOW (120ms in codex-rs
// tui/src/bottom_pane/paste_burst.rs, rust-v0.132.0) as a newline inside the
// paste rather than submit. 250ms clears that window with margin; claude only
// needs the CR to be a distinct write. Timing only — no output parsing.
const SUBMIT_GAP: Duration = Duration::from_millis(250);

/// Readiness gating for launch-prompt delivery. Wait for the harness to emit its
/// first PTY frame, then for its output to go quiet (the initial render
/// settling) before typing the prompt, so it lands in a ready input box rather
/// than being eaten by a redraw. All bounded so a silent or chatty harness still
/// gets its prompt.
const LAUNCH_FIRST_OUTPUT_TIMEOUT: Duration = Duration::from_secs(20);
const LAUNCH_QUIET_WINDOW: Duration = Duration::from_millis(600);
const LAUNCH_READY_MAX: Duration = Duration::from_secs(10);

/// Follow-up lone-Enter nudges after launch-prompt delivery, for harnesses that
/// swallow the submitting CR while still starting up (codex during MCP client
/// startup). Harmless when the prompt already submitted: Enter on an empty
/// composer does nothing in both claude and codex.
const LAUNCH_SUBMIT_NUDGES: [Duration; 2] = [Duration::from_secs(5), Duration::from_secs(10)];

/// claude launch-prompt delivery gate. claude 2.1.207's startup screen (the
/// welcome box + "connecting…" phase) swallows typed input during a multi-second
/// window that produces NO distinguishing PTY output -- output is quiet the whole
/// time -- so the timing heuristic that works for codex delivers into the dead
/// composer and the prompt is lost (tokens_in stays 0). There is also no
/// post-connecting "ready" event to wait for. So for claude we observe the actual
/// OUTCOME instead of guessing readiness: deliver, wait for the harness to confirm
/// the submission (its `UserPromptSubmit` hook -> `notify_prompt_accepted`), and
/// redeliver if it did not land. This is confirmation only; no TUI text is parsed.
///
/// Floor before the first attempt: don't type before the composer even exists.
/// claude's `SessionStart` hook fires as the session comes up; bounded so a
/// hookless/broken setup still proceeds into the retry loop (which self-corrects).
const SESSION_READY_TIMEOUT: Duration = Duration::from_secs(30);
/// Redelivery cap and spacing. Two opposing failure modes bound these numbers.
/// Too-fast redelivery duplicates work: the UserPromptSubmit confirmation hook is
/// ASYNC by design (a synchronous hook would stall every user turn on the 10s
/// hook timeout whenever the daemon is slow or down), so a prompt that LANDED but
/// whose confirmation is still in flight is indistinguishable from a swallowed
/// one, and redelivering would type the task into a live composer a second time.
/// Too-slow gives up while claude is still connecting. The observed 2.1.207
/// swallow window is ~9s, so a 15s interval puts attempt 2 comfortably past it,
/// while a healthy confirmation over the local unix socket arrives in well under
/// 15s -- a landed prompt is confirmed long before the loop could redeliver. 3
/// attempts (~45s budget) caps the pathological worst case (confirmations slower
/// than 15s or dropped) at 2 duplicate submissions rather than 5.
const MAX_LAUNCH_ATTEMPTS: u32 = 3;
const LAUNCH_RETRY_INTERVAL: Duration = Duration::from_secs(15);

/// Per-node launch-prompt delivery signals, posted by the capability service from
/// claude's hook events. Only created for a claude node that has a launch prompt.
#[derive(Default)]
pub(crate) struct LaunchSignals {
    /// Fired when claude's `SessionStart` hook posts (the session/composer is
    /// coming up). Gates the first delivery attempt.
    session_started: Notify,
    /// Fired when claude's `UserPromptSubmit` hook posts (a prompt was actually
    /// submitted). Wakes the retry loop so it stops redelivering.
    prompt_accepted: Notify,
    /// Latches true on the first acceptance so the retry loop never redelivers
    /// after the prompt has landed, closing the notify/wait race.
    accepted: AtomicBool,
}

/// Addendum B: on stop, claude nodes are asked to quit cleanly (the `/exit`
/// slash command) so the harness flushes its session transcript before we fall
/// back to killing the process. Bounded so a wedged harness still gets torn
/// down promptly.
const GRACEFUL_QUIT_WAIT: Duration = Duration::from_secs(5);
const GRACEFUL_QUIT_POLL: Duration = Duration::from_millis(100);
/// claude 2.1.202 clean-quit slash command (verified via docs: `/quit` aliases
/// `/exit`). Delivered as a submitted message (body + CR) like any TUI input.
const CLAUDE_QUIT_COMMAND: &str = "/exit";

/// Deliver `text` to a writer as a SUBMITTED message: write the body, pause, then
/// write a lone carriage return as a DISTINCT write. Claude's TUI uses auto-paste
/// / bracketed-paste detection, so a CR bundled into the same write as the body
/// is absorbed as pasted content and never submits; only a CR arriving as its own
/// keystroke submits. codex additionally suppresses Enter-as-submit for 120ms
/// after a paste burst ends, so the gap before the CR must exceed that window
/// (live-verified: 50ms left the prompt sitting in codex's composer).
async fn submit_over_writer(
    submit_lock: &Arc<Mutex<()>>,
    writer: &Arc<Mutex<Box<dyn Write + Send>>>,
    text: &str,
) -> Result<()> {
    // M5: hold a per-node submit lock across the ENTIRE body -> gap -> CR
    // sequence. The writer mutex alone is insufficient: it is released during
    // SUBMIT_GAP, so two concurrent submits would interleave as
    // body_a, body_b, CR, CR -- both bodies land in the composer and the first
    // CR submits the concatenation. Serializing the whole sequence makes each
    // submit atomic relative to every other submit on the same node.
    let _submit = submit_lock.lock().await;
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

/// Wait for the harness TUI's initial render to settle: first PTY frame seen,
/// then output quiet for a full window. No output content is inspected — this is
/// timing only, keeping Asylum as dumb plumbing. Bounded so a silent or
/// continuously-redrawing harness still proceeds.
async fn await_first_output_and_settle(rx: &mut broadcast::Receiver<String>) {
    // First frame (bounded: proceed anyway if the harness never prints).
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
}

/// codex launch-prompt delivery: settle on the initial render, deliver, then fire
/// a couple of lone-Enter nudges. Unchanged behaviour — codex has no session/
/// acceptance hooks, and its timing heuristic works because its startup does not
/// swallow a *delivered body* the way claude 2.1.207's connecting screen does.
async fn await_ready_and_deliver_codex(
    submit_lock: Arc<Mutex<()>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    mut rx: broadcast::Receiver<String>,
    prompt: String,
    node_id: Uuid,
) {
    await_first_output_and_settle(&mut rx).await;
    match submit_over_writer(&submit_lock, &writer, &prompt).await {
        Ok(()) => tracing::debug!(node_id = %node_id, "delivered launch prompt"),
        Err(e) => {
            tracing::warn!(node_id = %node_id, error = %e, "launch prompt delivery failed")
        }
    }
    // Launch-only submit nudges: codex swallows Enter-as-submit while its MCP
    // startup phase is still running (live-verified: the typed packet sits in
    // the composer if the CR lands during startup, and submits fine after). A
    // lone CR on codex's empty composer is a no-op once the prompt has submitted.
    for delay in LAUNCH_SUBMIT_NUDGES {
        tokio::time::sleep(delay).await;
        let _submit = submit_lock.lock().await;
        let mut w = writer.lock().await;
        if w.write_all(b"\r").and_then(|_| w.flush()).is_err() {
            break;
        }
    }
}

/// claude launch-prompt delivery: wait for the session-up floor, let the initial
/// render settle, then deliver-and-confirm with redelivery (see `LaunchSignals`
/// and `SESSION_READY_TIMEOUT`). This replaces the pure timing heuristic, which
/// races claude 2.1.207's input-swallowing connecting screen.
async fn await_ready_and_deliver_claude(
    signals: Arc<LaunchSignals>,
    submit_lock: Arc<Mutex<()>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    mut rx: broadcast::Receiver<String>,
    prompt: String,
    node_id: Uuid,
) {
    // Floor: don't type before the composer exists.
    let _ = tokio::time::timeout(SESSION_READY_TIMEOUT, signals.session_started.notified()).await;
    // Let the initial welcome render settle before the first attempt.
    await_first_output_and_settle(&mut rx).await;
    deliver_launch_prompt_with_retry(
        &signals,
        &submit_lock,
        &writer,
        &prompt,
        node_id,
        MAX_LAUNCH_ATTEMPTS,
        LAUNCH_RETRY_INTERVAL,
    )
    .await;
}

/// Deliver the launch prompt, redelivering until the harness confirms a prompt
/// submission (`signals.accepted`, set from claude's `UserPromptSubmit` hook) or
/// the attempt budget is exhausted. Confirmation-driven, not text-parsing: we
/// observe whether the prompt actually landed and retry if not, which is the only
/// robust gate for claude 2.1.207's connecting screen (it swallows input during a
/// silent window no timing heuristic can detect).
///
/// Accepted bounded behaviour: the confirmation is ANY UserPromptSubmit, not
/// specifically ours -- correlating a confirmation to a specific submission would
/// require parsing TUI output, which is barred (dumb plumbing). So an operator
/// who types into a just-launched node during the retry window takes over: their
/// submission stops the loop even if the launch prompt itself never landed. The
/// acceptance-exit log below makes that narrow case diagnosable.
async fn deliver_launch_prompt_with_retry(
    signals: &LaunchSignals,
    submit_lock: &Arc<Mutex<()>>,
    writer: &Arc<Mutex<Box<dyn Write + Send>>>,
    prompt: &str,
    node_id: Uuid,
    max_attempts: u32,
    retry_interval: Duration,
) {
    for attempt in 0..max_attempts {
        // A prior attempt may have landed while we were between iterations.
        if signals.accepted.load(Ordering::Acquire) {
            tracing::info!(
                node_id = %node_id,
                attempt,
                "launch prompt delivery loop stopped: a prompt submission was confirmed"
            );
            return;
        }
        if let Err(e) = submit_over_writer(submit_lock, writer, prompt).await {
            tracing::warn!(node_id = %node_id, error = %e, "launch prompt delivery failed");
            return;
        }
        if attempt == 0 {
            tracing::debug!(node_id = %node_id, attempt, "delivered launch prompt");
        } else {
            // Every redelivery is warn-logged: if the prior attempt actually
            // landed but its async confirmation was slow or dropped, this
            // redelivery duplicates the task in a live composer -- the log makes
            // any duplicate diagnosable from the daemon log alone.
            tracing::warn!(
                node_id = %node_id,
                attempt,
                "redelivered launch prompt (previous attempt unconfirmed)"
            );
        }
        // Wait for the harness to confirm the submission before redelivering.
        // notify_one stores a permit, so an acceptance that fires before this
        // await still wakes it; the accepted latch closes the remaining race.
        let _ = tokio::time::timeout(retry_interval, signals.prompt_accepted.notified()).await;
        if signals.accepted.load(Ordering::Acquire) {
            tracing::info!(
                node_id = %node_id,
                attempt,
                "launch prompt delivery loop stopped: a prompt submission was confirmed"
            );
            return;
        }
    }
    tracing::warn!(
        node_id = %node_id,
        attempts = max_attempts,
        "launch prompt not confirmed after retry budget"
    );
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
    /// M5: per-node lock held across a whole submit sequence so concurrent
    /// send_input calls cannot interleave their body/CR writes.
    submit_lock: Arc<Mutex<()>>,
    /// Addendum B: the harness kind, so stop() can send the harness's clean-quit
    /// command (claude `/exit`) before falling back to SIGKILL.
    harness: HarnessKind,
    /// Launch-prompt delivery signals, present only for a claude node launched
    /// with a prompt. The capability service posts SessionStart / UserPromptSubmit
    /// hook events onto these to gate and confirm delivery.
    launch_signals: Option<Arc<LaunchSignals>>,
}

/// How a local harness process ended, reported to the exit sink so the
/// capability service can distinguish a clean exit (`node.exited`) from an
/// abnormal one (`node.errored`).
#[derive(Clone, Copy, Debug)]
pub struct ExitOutcome {
    pub success: bool,
    pub code: Option<u32>,
    /// True when the exit signal was LOST (e.g. a loon SSE exec-stream errored or
    /// closed without an exit_code frame) rather than an authoritative exec exit.
    /// The exit sink maps this to node.errored/"stream_lost" (never a clean-exit
    /// lie), and the loon exit path must NOT tear the VM down on it (C1). Always
    /// false for the local substrate, whose child.wait() is authoritative.
    pub stream_lost: bool,
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

    /// Clone the launch-delivery signals for a node, if it has any (claude nodes
    /// launched with a prompt). Used by the capability service to route claude
    /// hook events into the delivery task.
    async fn launch_signals_for(&self, node_id: Uuid) -> Option<Arc<LaunchSignals>> {
        self.runtimes
            .read()
            .await
            .get(&node_id)
            .and_then(|runtime| runtime.launch_signals.clone())
    }

    /// Signal that claude's `SessionStart` hook fired for `node_id`: the session/
    /// composer is coming up, so the delivery task may make its first attempt.
    /// No-op for nodes without launch signals (codex, no-prompt, already gone).
    pub async fn notify_session_started(&self, node_id: Uuid) {
        if let Some(signals) = self.launch_signals_for(node_id).await {
            signals.session_started.notify_one();
        }
    }

    /// Signal that claude's `UserPromptSubmit` hook fired for `node_id`: a prompt
    /// was actually submitted, so the delivery task should stop redelivering. The
    /// latch closes the notify/wait race. No-op for nodes without launch signals.
    /// This is ANY prompt submission, not specifically the launch prompt's --
    /// see the accepted-bounded-behaviour note on
    /// `deliver_launch_prompt_with_retry` (operator takeover during the retry
    /// window stops the loop).
    pub async fn notify_prompt_accepted(&self, node_id: Uuid) {
        if let Some(signals) = self.launch_signals_for(node_id).await {
            signals.accepted.store(true, Ordering::Release);
            signals.prompt_accepted.notify_one();
        }
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
        let submit_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        let (output_tx, _) = broadcast::channel(1024);
        let output_tx_for_reader = output_tx.clone();

        // If a launch prompt is set, capture a writer handle and an output
        // subscription now — before `output_tx`/`writer_arc` are moved into the
        // runtime — so a background task can deliver the prompt as a submitted
        // message once the TUI is ready (bug fix: a positional prompt argv is
        // never auto-submitted by interactive harnesses).
        let launch_delivery = ctx.launch_prompt.clone().filter(|p| !p.is_empty()).map(
            |prompt| {
                (
                    prompt,
                    output_tx.subscribe(),
                    writer_arc.clone(),
                    submit_lock.clone(),
                )
            },
        );
        // claude nodes with a launch prompt use hook-confirmed delivery (see
        // `LaunchSignals`); the capability service posts SessionStart /
        // UserPromptSubmit onto these to gate and confirm delivery. Other
        // harnesses (codex) use the timing heuristic and need no signals.
        let launch_signals: Option<Arc<LaunchSignals>> =
            if launch_delivery.is_some() && matches!(ctx.harness, HarnessKind::ClaudeCode) {
                Some(Arc::new(LaunchSignals::default()))
            } else {
                None
            };

        // Insert into runtimes before spawning the reader task so that any
        // output emitted immediately on startup is not dropped (L3).
        self.runtimes.write().await.insert(
            node_id,
            LocalRuntime {
                writer: writer_arc,
                output_tx,
                killer: Arc::new(Mutex::new(killer)),
                submit_lock,
                harness: ctx.harness.clone(),
                launch_signals: launch_signals.clone(),
            },
        );

        if let Some((prompt, rx, writer, submit_lock)) = launch_delivery {
            match (&ctx.harness, launch_signals) {
                // claude: hook-confirmed, self-correcting delivery (fixes the
                // 2.1.207 connecting-screen swallow).
                (HarnessKind::ClaudeCode, Some(signals)) => {
                    tokio::spawn(await_ready_and_deliver_claude(
                        signals,
                        submit_lock,
                        writer,
                        rx,
                        prompt,
                        node_id,
                    ));
                }
                // codex (and any harness without launch signals): timing
                // heuristic + submit nudges, unchanged.
                _ => {
                    tokio::spawn(await_ready_and_deliver_codex(
                        submit_lock,
                        writer,
                        rx,
                        prompt,
                        node_id,
                    ));
                }
            }
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
                    stream_lost: false,
                },
                Err(_) => ExitOutcome {
                    success: false,
                    code: None,
                    stream_lost: false,
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
        let (writer, submit_lock) = self.writer_and_submit_lock_for(node_id).await?;
        submit_over_writer(&submit_lock, &writer, text).await
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

    /// Select the option at 0-based `option_index` in the harness's currently
    /// displayed single-select menu (e.g. claude AskUserQuestion). Delivers the
    /// down-arrow navigation and the submit CR as distinct paced PTY writes (see
    /// `menu_selection_writes`), holding the per-node submit lock across the whole
    /// sequence so a concurrent submit cannot interleave its own keystrokes. This
    /// is the typed-delivery path decision resolution uses instead of free-text +
    /// Enter, which would land on the default option.
    pub async fn send_menu_selection(&self, node_id: Uuid, option_index: usize) -> Result<()> {
        let (writer, submit_lock) = self.writer_and_submit_lock_for(node_id).await?;
        let _submit = submit_lock.lock().await;
        for write in crate::substrate::menu_selection_writes(option_index) {
            {
                let mut w = writer.lock().await;
                w.write_all(&write)?;
                w.flush()?;
            }
            // Gap between keystrokes: claude's TUI treats a bundled burst as a
            // paste, so the navigation keys and the submit CR must each arrive as
            // their own keypress (same reasoning as the text-submit gap).
            tokio::time::sleep(SUBMIT_GAP).await;
        }
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

    /// Clone both the shared writer and the per-node submit lock (M5), releasing
    /// the runtimes map lock before the caller runs the submit sequence.
    async fn writer_and_submit_lock_for(
        &self,
        node_id: Uuid,
    ) -> Result<(Arc<Mutex<Box<dyn Write + Send>>>, Arc<Mutex<()>>)> {
        let runtimes = self.runtimes.read().await;
        let runtime = runtimes
            .get(&node_id)
            .ok_or_else(|| anyhow!("node not running"))?;
        Ok((runtime.writer.clone(), runtime.submit_lock.clone()))
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
        // Clone the runtime handles WITHOUT removing the entry yet: the graceful
        // path needs the process to exit on its own so the reader task records the
        // real (clean) exit outcome and removes the runtime itself.
        let runtime = { self.runtimes.read().await.get(&node_id).cloned() };
        let Some(runtime) = runtime else {
            return Ok(());
        };

        // Addendum B: claude flushes its session transcript on a clean quit, so
        // ask it to `/exit` first and give the process a bounded window to leave
        // before we resort to SIGKILL (which would drop the transcript tail).
        // Codex has no cheap in-band clean-quit that reliably terminates the
        // process from a single PTY write (its `/quit` needs a confirm and a
        // stray CR can select a dialog default), so codex stays on the direct
        // kill path below -- documented here rather than guessed at.
        if matches!(runtime.harness, HarnessKind::ClaudeCode) {
            let _ = submit_over_writer(&runtime.submit_lock, &runtime.writer, CLAUDE_QUIT_COMMAND)
                .await;
            let deadline = tokio::time::Instant::now() + GRACEFUL_QUIT_WAIT;
            while tokio::time::Instant::now() < deadline {
                // The reader task removes the runtime from the map when the child
                // exits; a miss means the clean quit worked and no kill is needed.
                if !self.runtimes.read().await.contains_key(&node_id) {
                    return Ok(());
                }
                tokio::time::sleep(GRACEFUL_QUIT_POLL).await;
            }
        }

        // Fallback (non-claude, or claude that did not quit in time): remove the
        // runtime and SIGKILL the child.
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

    /// Guard on the retry budget's duplication bound. The UserPromptSubmit
    /// confirmation is async, so a too-short retry interval redelivers a prompt
    /// that already landed (duplicate task execution); the interval must clear
    /// both the observed 2.1.207 connecting window (~9s) and a slow-confirm
    /// margin, and the attempt cap bounds worst-case duplicates at 2. Retuning
    /// these numbers should be a conscious act against this contract.
    #[test]
    fn launch_retry_budget_bounds_duplicate_submissions() {
        assert!(
            LAUNCH_RETRY_INTERVAL >= Duration::from_secs(15),
            "retry interval must comfortably outlast the ~9s connecting window \
             and in-flight async confirmations"
        );
        assert!(
            MAX_LAUNCH_ATTEMPTS <= 3,
            "attempt cap bounds worst-case duplicate submissions at MAX-1"
        );
    }

    #[tokio::test]
    async fn submit_sequence_writes_body_then_a_distinct_carriage_return() {
        let recorder = RecordingWriter::default();
        let writes = recorder.writes.clone();
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(recorder)));

        let submit_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        submit_over_writer(&submit_lock, &writer, "Reply with exactly: SEND-OK")
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

        let submit_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        submit_over_writer(&submit_lock, &writer, "hello").await.unwrap();

        let recorded = writes.lock().unwrap();
        assert!(
            !recorded[0].contains(&b'\r'),
            "body write must not contain a carriage return"
        );
    }

    /// M5: two concurrent submits sharing one node's writer + submit lock must
    /// NOT interleave. Without the per-node submit lock the writes would come out
    /// as body_a, body_b, CR, CR (both bodies concatenated, one stray Enter);
    /// with it each submit's [body, CR] pair stays contiguous.
    #[tokio::test]
    async fn concurrent_submits_do_not_interleave_body_and_cr() {
        let recorder = RecordingWriter::default();
        let writes = recorder.writes.clone();
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(recorder)));
        let submit_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));

        let (w1, l1) = (writer.clone(), submit_lock.clone());
        let (w2, l2) = (writer.clone(), submit_lock.clone());
        let a = tokio::spawn(async move { submit_over_writer(&l1, &w1, "AAAA").await });
        let b = tokio::spawn(async move { submit_over_writer(&l2, &w2, "BBBB").await });
        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();

        let recorded = writes.lock().unwrap();
        assert_eq!(recorded.len(), 4, "expected body,CR,body,CR; got {recorded:?}");
        // Each submit must be a contiguous [body, CR] pair; neither ordering is
        // interleaved. Whichever ran first, its body is immediately followed by
        // its own CR.
        assert_eq!(recorded[1], b"\r", "first submit's CR must follow its body");
        assert_eq!(recorded[3], b"\r", "second submit's CR must follow its body");
        assert!(
            recorded[0] != b"\r" && recorded[2] != b"\r",
            "bodies must occupy the non-CR slots: {recorded:?}"
        );
        // And the two bodies are the two distinct messages, never merged.
        let mut bodies = vec![recorded[0].clone(), recorded[2].clone()];
        bodies.sort();
        assert_eq!(bodies, vec![b"AAAA".to_vec(), b"BBBB".to_vec()]);
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

    fn recording_writer() -> (
        Arc<Mutex<Box<dyn Write + Send>>>,
        Arc<StdMutex<Vec<Vec<u8>>>>,
    ) {
        let recorder = RecordingWriter::default();
        let writes = recorder.writes.clone();
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(recorder)));
        (writer, writes)
    }

    /// Retry core: once the harness confirms acceptance, delivery stops after the
    /// single landing attempt (body + CR = two writes) — no redelivery.
    #[tokio::test]
    async fn retry_delivers_once_when_prompt_is_accepted() {
        let signals = Arc::new(LaunchSignals::default());
        let (writer, writes) = recording_writer();
        let submit_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));

        // Confirm acceptance shortly after the first delivery.
        let confirm = signals.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            confirm.accepted.store(true, Ordering::Release);
            confirm.prompt_accepted.notify_one();
        });

        deliver_launch_prompt_with_retry(
            &signals,
            &submit_lock,
            &writer,
            "GO",
            Uuid::new_v4(),
            5,
            Duration::from_millis(500),
        )
        .await;

        let recorded = writes.lock().unwrap();
        assert_eq!(
            recorded.len(),
            2,
            "an accepted prompt is delivered exactly once (body, CR); got {recorded:?}"
        );
        assert_eq!(recorded[0], b"GO");
        assert_eq!(recorded[1], b"\r");
    }

    /// Retry core: when the prompt is never confirmed (e.g. hooks broken), it is
    /// redelivered up to the attempt budget — the fallback that guarantees no
    /// prompt is silently dropped.
    #[tokio::test]
    async fn retry_redelivers_up_to_budget_when_never_accepted() {
        let signals = Arc::new(LaunchSignals::default());
        let (writer, writes) = recording_writer();
        let submit_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));

        deliver_launch_prompt_with_retry(
            &signals,
            &submit_lock,
            &writer,
            "GO",
            Uuid::new_v4(),
            3,
            Duration::from_millis(10),
        )
        .await;

        let recorded = writes.lock().unwrap();
        // 3 attempts * (body, CR) = 6 writes.
        assert_eq!(
            recorded.len(),
            6,
            "unconfirmed prompt should retry to the budget; got {recorded:?}"
        );
    }

    /// Retry core: a prompt already confirmed before the loop starts (acceptance
    /// raced ahead of the first attempt) is never delivered — no double submit.
    #[tokio::test]
    async fn retry_delivers_nothing_when_already_accepted() {
        let signals = Arc::new(LaunchSignals::default());
        signals.accepted.store(true, Ordering::Release);
        let (writer, writes) = recording_writer();
        let submit_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));

        deliver_launch_prompt_with_retry(
            &signals,
            &submit_lock,
            &writer,
            "GO",
            Uuid::new_v4(),
            3,
            Duration::from_millis(10),
        )
        .await;

        assert!(
            writes.lock().unwrap().is_empty(),
            "an already-accepted prompt must not be delivered"
        );
    }

    /// End-to-end claude delivery gating: a claude node launched with a prompt
    /// does NOT deliver until `notify_session_started` fires (the composer-up
    /// floor), then delivers over the PTY. `notify_prompt_accepted` stops the
    /// retry loop. Proves the session gate and the substrate signal wiring.
    #[tokio::test]
    async fn claude_launch_waits_for_session_start_then_delivers() {
        let collected: Arc<StdMutex<String>> = Arc::new(StdMutex::new(String::new()));
        let sink = collected.clone();
        let substrate = LocalSubstrate::new(move |_id, text| {
            sink.lock().unwrap().push_str(text);
        });

        let node_id = Uuid::new_v4();
        let ctx = crate::substrate::SubstrateContext {
            node_id,
            harness: HarnessKind::ClaudeCode,
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "printf 'READY'; sleep 30".to_string()],
            workspace: None,
            env: Vec::new(),
            launch_prompt: Some("ASYLUM-CLAUDE-MARKER".to_string()),
        };
        substrate.launch(ctx).await.expect("launch should succeed");

        // Before SessionStart the delivery task is parked on the floor gate, so
        // nothing is typed into the (still-connecting) composer.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(
            !collected.lock().unwrap().contains("ASYLUM-CLAUDE-MARKER"),
            "claude prompt must not be delivered before session_started; captured: {:?}",
            collected.lock().unwrap()
        );

        // Session comes up: delivery proceeds after the render settles.
        substrate.notify_session_started(node_id).await;

        let mut delivered = false;
        for _ in 0..240 {
            if collected.lock().unwrap().contains("ASYLUM-CLAUDE-MARKER") {
                delivered = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        // Confirm acceptance so the retry loop stops cleanly.
        substrate.notify_prompt_accepted(node_id).await;
        let _ = substrate.stop(node_id).await;
        assert!(
            delivered,
            "claude launch prompt was not delivered after session_started; captured: {:?}",
            collected.lock().unwrap()
        );
    }

    /// Addendum B: stopping a claude node writes the clean-quit command over the
    /// PTY and lets the process exit on its own (flushing its transcript) before
    /// the SIGKILL fallback. The stand-in process reads one line and exits 0 iff
    /// it received `/exit`; had stop() gone straight to SIGKILL, the process
    /// would have been killed mid-read and the outcome would not be a clean
    /// self-exit (code 0).
    #[tokio::test]
    async fn claude_stop_sends_exit_command_and_lets_process_quit_cleanly() {
        let outcome: Arc<StdMutex<Option<ExitOutcome>>> = Arc::new(StdMutex::new(None));
        let outcome_sink = outcome.clone();
        let substrate = LocalSubstrate::new_with_sinks(
            |_id, _text| {},
            |_id, _req| {},
            move |_id, o| {
                *outcome_sink.lock().unwrap() = Some(o);
            },
        );

        let node_id = Uuid::new_v4();
        let ctx = crate::substrate::SubstrateContext {
            node_id,
            harness: HarnessKind::ClaudeCode,
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "IFS= read -r line; case \"$line\" in */exit*) exit 0;; *) exit 3;; esac"
                    .to_string(),
            ],
            workspace: None,
            env: Vec::new(),
            launch_prompt: None,
        };
        substrate.launch(ctx).await.expect("launch should succeed");
        // Let the shell reach its read() before we stop it.
        tokio::time::sleep(Duration::from_millis(300)).await;
        substrate.stop(node_id).await.expect("stop should succeed");

        let mut got = None;
        for _ in 0..100 {
            if let Some(o) = *outcome.lock().unwrap() {
                got = Some(o);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let got = got.expect("exit outcome should have been recorded");
        assert!(
            got.success && got.code == Some(0),
            "claude stop should quit the process cleanly via /exit (expected success exit 0), got {got:?}"
        );
    }
}
