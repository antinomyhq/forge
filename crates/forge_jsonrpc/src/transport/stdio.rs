use std::sync::Arc;

use jsonrpsee::server::RpcModule;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
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

        while let Ok(Some(line)) = lines.next_line().await {
            trace!("Received line: {}", line);

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

            match self
                .module
                .raw_json_request(&request_str, 1024 * 1024)
                .await
            {
                Ok((response_json, mut rx)) => {
                    // Write the initial response
                    let response: Value = serde_json::from_str(&response_json)
                        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;
                    Self::write_response(&stdout, &response).await?;

                    // Forward any subscription notifications to stdout
                    let stdout_clone = Arc::clone(&stdout);
                    tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Some(notification) => {
                                    match serde_json::from_str::<Value>(&notification) {
                                        Ok(notification_value) => {
                                            if let Err(e) =
                                                Self::write_response(&stdout_clone, &notification_value).await
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
                    });
                }
                Err(e) => {
                    error!("Failed to execute request: {}", e);
                    let id = request.get("id").cloned();
                    let error_response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32603,
                            "message": format!("Internal error: {}", e)
                        }
                    });
                    Self::write_response(&stdout, &error_response).await?;
                }
            }
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
