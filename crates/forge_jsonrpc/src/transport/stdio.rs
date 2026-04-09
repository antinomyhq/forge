use std::sync::Arc;

use jsonrpsee::server::RpcModule;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, trace};

/// STDIO transport for JSON-RPC
/// Reads JSON-RPC requests from stdin and writes responses to stdout
/// Directly executes methods using the RpcModule without any TCP server
pub struct StdioTransport {
    module: RpcModule<()>,
}

impl StdioTransport {
    pub fn new(module: RpcModule<()>) -> Self {
        Self { module }
    }

    /// Run the STDIO transport loop
    pub async fn run(self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let lines = reader.lines();
        let stdout = Arc::new(Mutex::new(stdout));

        debug!("STDIO transport started (direct mode, no TCP)");

        let mut lines = lines;
        let mut pending_tasks: Vec<JoinHandle<()>> = Vec::new();

        while let Ok(Some(line)) = lines.next_line().await {
            trace!("Received line: {}", line);

            // Clean up completed tasks
            pending_tasks.retain(|h| !h.is_finished());

            // Parse the JSON-RPC request
            let request: Value = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    error!("Failed to parse JSON-RPC request: {}", e);
                    let error_response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {
                            "code": -32700,
                            "message": format!("Parse error: {}", e)
                        }
                    });
                    Self::write_response(&stdout, &error_response).await?;
                    continue;
                }
            };

            // Execute the request
            let request_str = serde_json::to_string(&request)?;
            let module = self.module.clone();
            let stdout_clone = Arc::clone(&stdout);

            // Spawn the entire request handling so stdin can continue
            // reading (or reach EOF) without blocking on the response.
            let handle = tokio::spawn(async move {
                match module.raw_json_request(&request_str, 1024 * 1024).await {
                    Ok((response_json, mut rx)) => {
                        // Write the initial response
                        match serde_json::from_str::<Value>(&response_json) {
                            Ok(response) => {
                                if let Err(e) = Self::write_response(&stdout_clone, &response).await
                                {
                                    error!("Failed to write response: {}", e);
                                    return;
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse response: {}", e);
                                return;
                            }
                        }

                        // Forward any subscription notifications to stdout
                        loop {
                            match rx.recv().await {
                                Some(notification) => {
                                    match serde_json::from_str::<Value>(&notification) {
                                        Ok(notification_value) => {
                                            if let Err(e) = Self::write_response(
                                                &stdout_clone,
                                                &notification_value,
                                            )
                                            .await
                                            {
                                                error!("Failed to write notification: {}", e);
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to parse notification: {}", e);
                                        }
                                    }
                                }
                                None => break,
                            }
                        }
                        debug!("Notification stream ended");
                    }
                    Err(e) => {
                        error!("Failed to execute request: {}", e);
                        let error_response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": null,
                            "error": {
                                "code": -32603,
                                "message": format!("Internal error: {}", e)
                            }
                        });
                        let _ = Self::write_response(&stdout_clone, &error_response).await;
                    }
                }
            });
            pending_tasks.push(handle);
        }

        // Wait for all in-flight requests and their notification
        // streams to finish before exiting.
        debug!(
            "STDIO stdin closed, waiting for {} pending task(s)",
            pending_tasks.len()
        );
        for handle in pending_tasks {
            let _ = handle.await;
        }

        debug!("STDIO transport stopped");
        Ok(())
    }

    /// Write a JSON-RPC response to stdout
    async fn write_response(
        writer: &Arc<Mutex<tokio::io::Stdout>>,
        response: &Value,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(response)?;
        trace!("Sending response: {}", json);

        let mut guard = writer.lock().await;
        guard.write_all(json.as_bytes()).await?;
        guard.write_all(b"\n").await?;
        guard.flush().await?;

        Ok(())
    }
}
