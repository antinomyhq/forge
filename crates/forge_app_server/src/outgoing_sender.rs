use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::protocol::{JsonRpcNotification, JsonRpcResponse, ServerNotification};

/// Handles sending JSON-RPC messages to the client via stdout
pub struct OutgoingMessageSender<W: AsyncWrite + Unpin + Send> {
    writer: Arc<Mutex<W>>,
}

impl<W: AsyncWrite + Unpin + Send> Clone for OutgoingMessageSender<W> {
    fn clone(&self) -> Self {
        Self { writer: self.writer.clone() }
    }
}

impl<W: AsyncWrite + Unpin + Send> OutgoingMessageSender<W> {
    /// Creates a new OutgoingMessageSender with the given writer
    pub fn new(writer: W) -> Self {
        Self { writer: Arc::new(Mutex::new(writer)) }
    }

    /// Sends a JSON-RPC response
    pub async fn send_response(&self, id: i64, result: impl Serialize) -> Result<()> {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        };

        self.write_message(&response).await
    }

    /// Sends a JSON-RPC error response
    pub async fn send_error(&self, id: i64, code: i32, message: &str) -> Result<()> {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(crate::protocol::JsonRpcError {
                code,
                message: message.to_string(),
                data: None,
            }),
        };

        self.write_message(&response).await
    }

    /// Sends a server notification
    pub async fn send_notification(&self, notification: ServerNotification) -> Result<()> {
        let method = Self::get_notification_method(&notification);

        // Serialize the notification to get its params
        // ServerNotification uses #[serde(tag = "method", content = "params")]
        // So we need to extract just the params part
        let full_json = serde_json::to_value(&notification)?;
        let params = full_json
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let notification_json = JsonRpcNotification { jsonrpc: "2.0".to_string(), method, params };

        self.write_message(&notification_json).await
    }

    /// Writes a message to stdout with newline delimiter
    async fn write_message(&self, message: &impl Serialize) -> Result<()> {
        let json = serde_json::to_string(message)?;
        let mut writer = self.writer.lock().await;

        tracing::debug!("Sending message: {}", json);

        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        Ok(())
    }

    /// Extracts the method name from a ServerNotification
    fn get_notification_method(notification: &ServerNotification) -> String {
        match notification {
            ServerNotification::ThreadStarted { .. } => "thread/started",
            ServerNotification::TurnStarted { .. } => "turn/started",
            ServerNotification::TurnCompleted { .. } => "turn/completed",
            ServerNotification::ItemStarted { .. } => "item/started",
            ServerNotification::ItemCompleted { .. } => "item/completed",
            ServerNotification::AgentMessageDelta { .. } => "item/agentMessage/delta",
            ServerNotification::AgentReasoningDelta { .. } => "item/agentReasoning/delta",
            ServerNotification::CommandExecutionOutput { .. } => "item/commandExecution/output",
            ServerNotification::TurnUsage { .. } => "turn/usage",
            ServerNotification::Progress { .. } => "progress",
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::BufWriter;

    use super::*;

    #[tokio::test]
    async fn test_send_response() {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);
        let sender = OutgoingMessageSender::new(writer);

        let result = sender
            .send_response(1, serde_json::json!({"success": true}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_error() {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);
        let sender = OutgoingMessageSender::new(writer);

        let result = sender.send_error(1, -32600, "Invalid Request").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_notification() {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);
        let sender = OutgoingMessageSender::new(writer);

        let notification = ServerNotification::AgentMessageDelta {
            thread_id: uuid::Uuid::new_v4(),
            turn_id: uuid::Uuid::new_v4(),
            item_id: uuid::Uuid::new_v4(),
            delta: "Hello".to_string(),
        };

        sender.send_notification(notification).await.unwrap();

        // Get the buffer content - need to get inner buffer
        let writer_guard = sender.writer.lock().await;
        let inner_buffer = writer_guard.get_ref();
        let buffer_content = String::from_utf8(inner_buffer.clone()).unwrap();

        println!("Buffer content: {:?}", buffer_content);

        // Remove trailing newline
        let buffer_content = buffer_content.trim();

        // Parse the JSON
        let json: serde_json::Value = serde_json::from_str(buffer_content).unwrap();

        // Verify structure - params should NOT be double-wrapped
        assert_eq!(json["method"], "item/agentMessage/delta");
        assert_eq!(json["params"]["delta"], "Hello");

        // Verify there's no double-wrapping
        assert!(json["params"]["method"].is_null());
        assert!(json["params"]["params"].is_null());
    }
}
