use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use forge_api::ForgeAPI;
use forge_config::ForgeConfig;
use forge_jsonrpc::JsonRpcServer;
use tracing::debug;
use url::Url;

/// Forge JSON-RPC Server
///
/// A JSON-RPC server for Forge that communicates over STDIO (stdin/stdout).
/// Uses newline-delimited JSON-RPC over stdio, suitable for programmatic integrations.
/// Note: This is standard JSON-RPC over stdio, not LSP (Language Server Protocol)
/// which uses Content-Length framing.
#[derive(Parser)]
#[command(name = "forge-jsonrpc")]
#[command(about = "JSON-RPC server for Forge (STDIO mode)")]
struct Cli {
    /// Working directory
    #[arg(short, long)]
    directory: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Set up panic hook for better error reporting
    std::panic::set_hook(Box::new(|info| {
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line()));
        if let Some(loc) = location {
            tracing::error!("Panic occurred at {}: {:?}", loc, info.payload());
        } else {
            tracing::error!("Panic occurred: {:?}", info.payload());
        }
    }));

    // Initialize tracing for logging
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    debug!("Starting Forge JSON-RPC server (STDIO mode)");

    let cli = Cli::parse();

    // Read configuration
    let config =
        ForgeConfig::read().context("Failed to read Forge configuration from .forge.toml")?;

    let services_url: Url = config
        .services_url
        .parse()
        .context("services_url in configuration must be a valid URL")?;

    let cwd = cli
        .directory
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    // Initialize the API
    let api = Arc::new(ForgeAPI::init(cwd, config, services_url));

    // Create and run the JSON-RPC server (STDIO mode only)
    let server = JsonRpcServer::new(api);
    server.run_stdio().await
}
