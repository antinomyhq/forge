use std::sync::Arc;

use anyhow::Result;
use forge_api::API;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{MessageProcessor, OutgoingMessageSender};

/// Main application server that handles JSON-RPC communication over stdio
pub struct AppServer<A> {
    api: Arc<A>,
}

impl<A: API + 'static> AppServer<A> {
    /// Creates a new AppServer with the given API instance
    pub fn new(api: A) -> Self {
        Self { api: Arc::new(api) }
    }

    /// Runs the server, processing JSON-RPC messages from stdin and writing
    /// responses to stdout
    pub async fn run(self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        let sender = OutgoingMessageSender::new(stdout);
        let processor = MessageProcessor::new(self.api.clone(), sender.clone());

        tracing::info!("App server ready, waiting for messages");

        // Process incoming JSON-RPC messages line by line
        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            tracing::debug!("Received message: {}", line);

            // Process the message
            if let Err(e) = processor.process_message(&line).await {
                tracing::error!("Error processing message: {}", e);
                // Send error response
                sender.send_error(-1, -32603, &e.to_string()).await?;
            }
        }

        tracing::info!("stdin closed, shutting down");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        use std::path::PathBuf;

        use forge_api::ForgeAPI;

        let api = ForgeAPI::init(false, PathBuf::from("."));
        let _server = AppServer::new(api);
    }
}
