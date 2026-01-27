//! ACP server implementation with stdio and HTTP transports.

use std::sync::Arc;

use agent_client_protocol as acp;
use forge_app::{ForgeApp, Services};
use tokio::sync::mpsc;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{ForgeAgent, Result};

/// Information about the ACP agent.
#[derive(Debug, Clone)]
pub struct AgentInfo {
    /// Agent name.
    pub name: String,
    /// Agent version.
    pub version: String,
    /// Agent capabilities.
    pub capabilities: String,
}

/// Starts an ACP server using stdio transport (for local agent mode).
///
/// This is the primary mode for IDE integration where the IDE spawns Forge
/// as a subprocess and communicates via stdin/stdout.
///
/// # Arguments
///
/// * `app` - The Forge application instance
///
/// # Errors
///
/// Returns an error if the server fails to start or encounters a fatal error.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use forge_app::ForgeApp;
/// use forge_acp::start_stdio_server;
///
/// # async fn example() -> anyhow::Result<()> {
/// let services = Arc::new(/* your services */);
/// let app = Arc::new(ForgeApp::new(services));
/// start_stdio_server(app).await?;
/// # Ok(())
/// # }
/// ```
pub async fn start_stdio_server<S: Services>(app: Arc<ForgeApp<S>>) -> Result<()> {
    tracing::info!("Starting ACP server with stdio transport");

    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    // Create channel for session notifications
    let (tx, mut rx) = mpsc::unbounded_channel();

    // The AgentSideConnection will spawn futures onto our Tokio runtime.
    // LocalSet and spawn_local are used because the futures from the
    // agent-client-protocol crate are not Send.
    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            let agent = ForgeAgent::new(app, tx);

            // Start up the ForgeAgent connected to stdio
            let (conn, handle_io) =
                acp::AgentSideConnection::new(agent, outgoing, incoming, |fut| {
                    tokio::task::spawn_local(fut);
                });

            // Kick off a background task to send session notifications to the client
            tokio::task::spawn_local(async move {
                while let Some(session_notification) = rx.recv().await {
                    if let Err(e) = conn.session_notification(session_notification).await {
                        tracing::error!("Failed to send session notification: {}", e);
                        break;
                    }
                }
            });

            // Run until stdin/stdout are closed
            handle_io.await
        })
        .await
        .map_err(|e| crate::Error::Application(anyhow::anyhow!("ACP server error: {}", e)))
}

/// Starts an ACP server using HTTP/WebSocket transport (for remote agent mode).
///
/// This mode allows Forge to run as a remote service that IDEs can connect to
/// over the network.
///
/// # Arguments
///
/// * `app` - The Forge application instance
/// * `port` - The port to listen on
///
/// # Errors
///
/// Returns an error if the server fails to start or bind to the port.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use forge_app::ForgeApp;
/// use forge_acp::start_http_server;
///
/// # async fn example() -> anyhow::Result<()> {
/// let services = Arc::new(/* your services */);
/// let app = Arc::new(ForgeApp::new(services));
/// start_http_server(app, 3000).await?;
/// # Ok(())
/// # }
/// ```
pub async fn start_http_server<S: Services>(_app: Arc<ForgeApp<S>>, port: u16) -> Result<()> {
    tracing::info!("Starting ACP server with HTTP transport on port {}", port);

    // TODO: Implement HTTP/WebSocket transport
    // This will require:
    // 1. HTTP server setup (e.g., using axum or warp)
    // 2. WebSocket upgrade handling
    // 3. JSON-RPC over WebSocket
    // 4. Session management for multiple concurrent clients

    Err(crate::Error::InvalidRequest(
        "HTTP transport not yet implemented".to_string(),
    ))
}

/// Returns information about the ACP agent capabilities.
///
/// This can be used to display agent information without starting the server.
pub fn agent_info() -> AgentInfo {
    AgentInfo {
        name: "forge".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        capabilities: "file_system, terminal, tools".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_agent_info() {
        let info = agent_info();
        assert_eq!(info.name, "forge");
        assert!(!info.version.is_empty());
        assert!(info.capabilities.contains("file_system"));
    }
}
