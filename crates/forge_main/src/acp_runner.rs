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
        // Initialize Forge infrastructure
        let infra = Arc::new(ForgeInfra::new(false, cwd));
        let repo = Arc::new(ForgeRepo::new(infra.clone()));
        let services = Arc::new(ForgeServices::new(repo.clone()));
        let app = Arc::new(ForgeApp::new(services));

        // Start the ACP server
        acp::start_stdio_server(app).await?;
        
        Ok(())
    })
}
