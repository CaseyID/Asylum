mod cli;
mod client;
mod host;
mod mcp;
mod native_attach;
mod runtime;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    cli::run().await
}
