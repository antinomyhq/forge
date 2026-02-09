//! ACP server runner using the ForgeAPI.
//!
//! The API layer handles the LocalSet setup internally, so this is just
//! a simple wrapper that initializes the API and calls acp_start_stdio().

use std::path::PathBuf;

use anyhow::Result;
use forge_api::{ForgeAPI, API};

/// Runs the ACP server in stdio mode.
///
/// This function initializes the ForgeAPI and starts the ACP server.
/// The API layer handles all the LocalSet and runtime setup internally.
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

    // Initialize the API
    let api = ForgeAPI::init(false, cwd);

    // Start the ACP server - the API handles LocalSet internally
    // This is a blocking call that returns when the server exits
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(api.acp_start_stdio())
}
