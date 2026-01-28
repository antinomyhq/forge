//! Standalone ACP server runner that bypasses the async API layer.
//!
//! This is necessary because the ACP SDK uses !Send futures which require
//! LocalSet, and we can't use LocalSet in the multi-threaded Tokio runtime used
//! by the main app.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use forge_app::ForgeApp;
use forge_infra::ForgeInfra;
use forge_repo::ForgeRepo;
use forge_services::ForgeServices;

/// Runs the ACP server in stdio mode using a single-threaded runtime.
///
/// This function creates its own runtime and blocks until the server exits.
/// It should be called directly from main() without going through the async
/// API.
pub fn run_acp_stdio_server(cwd: PathBuf) -> Result<()> {
    // Initialize tracing subscriber for ACP server logging
    // Logs will go to stderr (which the IDE should capture)
    // Log level can be configured via RUST_LOG environment variable (defaults to
    // INFO)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting Forge ACP server in stdio mode");

    // Create a single-threaded runtime for the ACP server
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async move {
        let local_set = tokio::task::LocalSet::new();

        local_set
            .run_until(async move {
                // Initialize Forge infrastructure
                let infra = Arc::new(ForgeInfra::new(false, cwd));
                let repo = Arc::new(ForgeRepo::new(infra.clone()));
                let services = Arc::new(ForgeServices::new(repo.clone()));
                let app = Arc::new(ForgeApp::new(services));

                // Set up signal handling for graceful shutdown
                let server_task = tokio::task::spawn_local(forge_acp::start_stdio_server(app));

                match server_task.await {
                    Ok(Ok(())) => {
                        tracing::info!("ACP server exited normally");
                        Ok(())
                    }
                    Ok(Err(e)) => {
                        tracing::error!("ACP server error: {}", e);
                        Err(e.into())
                    }
                    Err(e) => {
                        tracing::error!("Server task panicked: {}", e);
                        Err(e.into())
                    }
                }
            })
            .await
    })
}
