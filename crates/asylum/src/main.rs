use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::SocketAddr;

#[derive(Parser)]
#[command(name = "asylum", version = "0.1.0")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value = "127.0.0.1:7717")]
        bind: SocketAddr,
        #[arg(long, default_value = ".asylum/asylum.sqlite3")]
        database: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let args = Args::parse();
    match args.command {
        Command::Serve { bind, database } => asylum_daemon::app::serve(bind, database).await,
    }
}
