//! JSON-RPC server binary for the Forge API

use std::env;

use anyhow::Result;
use forge_server::{ForgeServer, ForgeServerConfig};
use tracing::{debug, info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with explicit stderr output
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_writer(std::io::stderr) // Make sure logs go to stderr, not stdout
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Forge JSON-RPC server");

    // Parse command line arguments
    let restricted = env::args().any(|arg| arg == "--restricted");
    debug!("Restricted mode: {}", restricted);

    // Initialize the server
    let config = ForgeServerConfig::new().with_restricted(restricted);

    let server = match ForgeServer::new(config) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("Failed to initialize Forge server: {}", e);
            std::process::exit(1);
        }
    };

    // Setup CTRL+C handler using tokio's signal handler
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Received Ctrl+C, initiating graceful shutdown");
                // In this implementation, we rely on the ForgeServer's internal
                // ctrl_c handler
            }
            Err(err) => {
                eprintln!("Error setting up Ctrl+C handler: {:?}", err);
            }
        }
    });

    // Start the server
    if let Err(e) = server.start() {
        eprintln!("Forge server encountered an error: {}", e);
        std::process::exit(1);
    }

    info!("Forge JSON-RPC server terminated");
    Ok(())
}
