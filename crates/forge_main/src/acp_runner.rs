//! Standalone ACP server runner that bypasses the async API layer.
//!
//! This is necessary because the ACP SDK uses !Send futures which require LocalSet,
//! and we can't use LocalSet in the multi-threaded Tokio runtime used by the main app.

use std::sync::Arc;
use anyhow::Result;
use forge_infra::ForgeInfra;
use forge_repo::{ForgeRepo, acp};
use forge_services::ForgeServices;
use forge_app::ForgeApp;
use std::path::PathBuf;

/// Runs the ACP server in stdio mode using a single-threaded runtime.
///
/// This function creates its own runtime and blocks until the server exits.
/// It should be called directly from main() without going through the async API.
pub fn run_acp_stdio_server(cwd: PathBuf) -> Result<()> {
    // Create a single-threaded runtime for the ACP server
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async move {
        let local_set = tokio::task::LocalSet::new();
        
        local_set.run_until(async move {
            // Initialize Forge infrastructure
            let infra = Arc::new(ForgeInfra::new(false, cwd));
            let repo = Arc::new(ForgeRepo::new(infra.clone()));
            let services = Arc::new(ForgeServices::new(repo.clone()));
            let app = Arc::new(ForgeApp::new(services));

            // Set up signal handling for graceful shutdown
            let mut server_task = tokio::task::spawn_local(async move {
                acp::start_stdio_server(app).await
            });

            // Wait for either the server to exit or a signal
            #[cfg(unix)]
            {
                let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
                let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
                
                tokio::select! {
                    result = &mut server_task => {
                        match result {
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
                    }
                    _ = sigterm.recv() => {
                        tracing::info!("Received SIGTERM, shutting down gracefully");
                        server_task.abort();
                        Ok(())
                    }
                    _ = sigint.recv() => {
                        tracing::info!("Received SIGINT, shutting down gracefully");
                        server_task.abort();
                        Ok(())
                    }
                }
            }

            #[cfg(not(unix))]
            {
                // On non-Unix systems, just wait for the server task
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
            }
        }).await
    })
}
