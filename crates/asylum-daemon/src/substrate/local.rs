use anyhow::{anyhow, Context, Result};
use asylum_core::node::{CapabilitySnapshot, HarnessKind};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use super::{SubstrateContext, SubstrateOutput};

#[derive(Clone)]
struct LocalRuntime {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

#[derive(Clone)]
pub struct LocalSubstrate {
    runtimes: Arc<RwLock<HashMap<Uuid, LocalRuntime>>>,
    output_sink: Arc<dyn SubstrateOutput>,
}

impl LocalSubstrate {
    pub fn new<F>(output_sink: F) -> Self
    where
        F: Fn(Uuid, &str) + Send + Sync + 'static,
    {
        Self {
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            output_sink: Arc::new(output_sink),
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
        let mut reader = pty.master.try_clone_reader()?;
        let writer = pty.master.take_writer()?;

        let node_id = ctx.node_id;
        let sink = self.output_sink.clone();
        let writer_arc: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(writer));

        tokio::task::spawn_blocking(move || {
            let mut local_child = child;
            let mut buffer = [0_u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        let chunk = String::from_utf8_lossy(&buffer[..size]).to_string();
                        sink(node_id, &chunk);
                    }
                    Err(_) => break,
                }
            }
            let _ = local_child.wait();
        });

        // keep the child alive by not dropping `child` here.
        // this is intentionally retained in thread scope only; we also keep output writer handle.
        self.runtimes
            .write()
            .await
            .insert(node_id, LocalRuntime { writer: writer_arc });
        Ok(())
    }

    pub async fn send_input(&self, node_id: Uuid, text: &str) -> Result<()> {
        let runtimes = self.runtimes.read().await;
        let runtime = runtimes
            .get(&node_id)
            .ok_or_else(|| anyhow!("node not running"))?;
        let mut writer = runtime.writer.lock().await;
        writer.write_all(text.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
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
        let mut runtimes = self.runtimes.write().await;
        runtimes.remove(&node_id);
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
