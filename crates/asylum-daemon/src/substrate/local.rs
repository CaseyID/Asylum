use anyhow::{anyhow, Context, Result};
use asylum_types::node::{CapabilitySnapshot, HarnessKind};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;

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

#[derive(Clone)]
pub struct LocalSubstrate {
    runtimes: Arc<RwLock<HashMap<Uuid, LocalRuntime>>>,
    output_sink: Arc<dyn Fn(Uuid, &str) + Send + Sync>,
    decision_sink: Arc<dyn Fn(Uuid, DecisionProtocolRequest) + Send + Sync>,
    exit_sink: Arc<dyn Fn(Uuid) + Send + Sync>,
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
            exit_sink: Arc::new(|_| {}),
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
            exit_sink: Arc::new(|_| {}),
        }
    }

    pub fn new_with_sinks<F, D, E>(output_sink: F, decision_sink: D, exit_sink: E) -> Self
    where
        F: Fn(Uuid, &str) + Send + Sync + 'static,
        D: Fn(Uuid, DecisionProtocolRequest) + Send + Sync + 'static,
        E: Fn(Uuid) + Send + Sync + 'static,
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
            let _ = local_child.wait();
            // Harness exited (either on its own or via stop()); drop the runtime
            // so the daemon's view stays consistent and notify the capability
            // service so it can transition liveness.
            let runtimes_clone = runtimes_for_exit.clone();
            tokio::runtime::Handle::current().spawn(async move {
                runtimes_clone.write().await.remove(&node_id);
            });
            (exit_sink)(node_id);
        });

        Ok(())
    }

    pub async fn send_input(&self, node_id: Uuid, text: &str) -> Result<()> {
        self.send_input_raw(node_id, &format!("{text}\r")).await
    }

    pub async fn send_input_raw(&self, node_id: Uuid, text: &str) -> Result<()> {
        let runtimes = self.runtimes.read().await;
        let runtime = runtimes
            .get(&node_id)
            .ok_or_else(|| anyhow!("node not running"))?;
        let mut writer = runtime.writer.lock().await;
        writer.write_all(text.as_bytes())?;
        writer.flush()?;
        Ok(())
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
