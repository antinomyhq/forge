//! ACP (Agent Communication Protocol) application orchestrator

use std::sync::Arc;

use anyhow::Result;

use crate::Services;

/// ACP (Agent Communication Protocol) application orchestrator
///
/// Responsible for starting and managing the ACP server that communicates
/// with IDEs via stdio or HTTP transports.
pub struct AcpApp<S> {
    services: Arc<S>,
}

impl<S: Services> AcpApp<S> {
    /// Creates a new ACP application orchestrator
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Starts the ACP server over stdio transport
    ///
    /// This runs in a blocking thread with a single-threaded runtime
    /// because the ACP SDK uses !Send futures.
    ///
    /// # Errors
    ///
    /// Returns an error if the ACP server fails to start or encounters
    /// a fatal error during execution.
    pub async fn start_stdio(&self) -> Result<()> {
        use agent_client_protocol as acp;
        use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

        // We need to use spawn_blocking because LocalSet is !Send
        // This runs the entire ACP server in a blocking thread with its own runtime
        let services = self.services.clone();
        let handle = tokio::task::spawn_blocking(move || {
            // Create a new single-threaded runtime for the ACP server
            // This is necessary because the ACP SDK uses !Send futures
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime");

            rt.block_on(async move {
                // Create channel for session notifications
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                // Create adapter
                let adapter = Arc::new(crate::acp::AcpAdapter::new(services, tx));

                // Start transport - this will set up the connection and call set_client_connection
                // We need to use LocalSet since ACP SDK uses !Send futures
                let local_set = tokio::task::LocalSet::new();
                local_set
                    .run_until(async move {
                        let outgoing = tokio::io::stdout().compat_write();
                        let incoming = tokio::io::stdin().compat();

                        // Create ACP connection
                        let (conn, handle_io) = acp::AgentSideConnection::new(
                            adapter.clone(),
                            outgoing,
                            incoming,
                            |fut| {
                                tokio::task::spawn_local(fut);
                            },
                        );

                        let conn = Arc::new(conn);

                        // Set the client connection on the adapter for RPC calls
                        adapter.set_client_connection(conn.clone()).await;

                        // Forward notifications to client
                        let conn_for_notifications = conn.clone();
                        let notification_task = tokio::task::spawn_local(async move {
                            let mut rx = rx;
                            while let Some(session_notification) = rx.recv().await {
                                use agent_client_protocol::Client;
                                if let Err(e) = conn_for_notifications
                                    .session_notification(session_notification)
                                    .await
                                {
                                    tracing::error!("Failed to send session notification: {}", e);
                                    break;
                                }
                            }
                        });

                        // Run until stdin/stdout are closed
                        let io_result = handle_io.await;

                        // Cancel the notification task
                        notification_task.abort();

                        io_result.map_err(|e| anyhow::anyhow!("ACP transport error: {}", e))
                    })
                    .await
            })
        });

        // Wait for the blocking task to complete
        match handle.await {
            Ok(result) => result,
            Err(e) if e.is_cancelled() => {
                tracing::info!("ACP server task was cancelled");
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("ACP server task panicked: {}", e)),
        }
    }

    /// Starts the ACP server over HTTP transport
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP server fails to start.
    pub async fn start_http(&self, _port: u16) -> Result<()> {
        anyhow::bail!("HTTP transport not yet implemented")
    }
}
