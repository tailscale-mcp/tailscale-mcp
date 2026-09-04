//! The `tailscale-mcp` binary.
//!
//! Everything interesting lives in the library; this is the wiring. The one
//! rule it exists to enforce is that standard output belongs to the protocol:
//! logs, startup notes and warnings all go to standard error, because a stray
//! line on standard output ends the session for a stdio client.

use anyhow::Context as _;
use clap::Parser as _;
use rmcp::ServiceExt as _;
use rmcp::transport::stdio;
use tailscale_mcp::config::{Cli, Config};
use tailscale_mcp::server::{self, Backends};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::resolve(cli)?;
    init_tracing(&config.log_filter);

    let backends = Backends::discover(&config);
    let startup = server::build(&config, tailscale_mcp::tools::entries(), backends).await?;
    for note in &startup.notes {
        eprintln!("tailscale-mcp: {note}");
    }

    let service = startup
        .server
        .serve(stdio())
        .await
        .context("could not start the stdio transport")?;
    service
        .waiting()
        .await
        .context("the session ended abnormally")?;
    Ok(())
}

/// Logging, on standard error and nowhere else.
fn init_tracing(filter: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
}
