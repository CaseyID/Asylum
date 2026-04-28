mod cli;
mod client;
mod mcp;
mod native_attach;
mod runtime;
mod service;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    cli::run().await
}
