use std::sync::Arc;

use anyhow::Result;

use crate::Services;

/// ACP (Agent Communication Protocol) application orchestrator.
pub struct AcpApp<S> {
    services: Arc<S>,
}

impl<S: Services> AcpApp<S> {
    /// Creates a new ACP application orchestrator.
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Starts the ACP server over stdio transport.
    pub async fn start_stdio(&self) -> Result<()> {
        use agent_client_protocol as acp;
        use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

        let services = self.services.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime");

            rt.block_on(async move {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let adapter = Arc::new(crate::acp::AcpAdapter::new(services, tx));

                let local_set = tokio::task::LocalSet::new();
                local_set
                    .run_until(async move {
                        let outgoing = tokio::io::stdout().compat_write();
                        let incoming = tokio::io::stdin().compat();

                        let (conn, handle_io) = acp::AgentSideConnection::new(
                            adapter.clone(),
                            outgoing,
                            incoming,
                            |fut| {
                                tokio::task::spawn_local(fut);
                            },
                        );

                        let conn = Arc::new(conn);
                        adapter.set_client_connection(conn.clone()).await;

                        let conn_for_notifications = conn.clone();
                        let notification_task = tokio::task::spawn_local(async move {
                            let mut rx = rx;
                            while let Some(session_notification) = rx.recv().await {
                                use agent_client_protocol::Client;

                                if let Err(error) = conn_for_notifications
                                    .session_notification(session_notification)
                                    .await
                                {
                                    tracing::error!(
                                        "Failed to send session notification: {}",
                                        error
                                    );
                                    break;
                                }
                            }
                        });

                        let io_result = handle_io.await;
                        notification_task.abort();

                        io_result.map_err(|error| anyhow::anyhow!("ACP transport error: {}", error))
                    })
                    .await
            })
        });

        match handle.await {
            Ok(result) => result,
            Err(error) if error.is_cancelled() => {
                tracing::info!("ACP server task was cancelled");
                Ok(())
            }
            Err(error) => Err(anyhow::anyhow!("ACP server task panicked: {}", error)),
        }
    }
}