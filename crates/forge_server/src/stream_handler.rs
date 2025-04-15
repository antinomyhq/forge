//! Stream handling for JSON-RPC

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use forge_domain::*;
use forge_stream::MpscStream;
use futures::StreamExt;
use jsonrpc_core::{Notification, Params, Value, Version};
use serde_json::json;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::serialization::chat_response_to_json;

/// Manages active streams
#[derive(Default)]
pub struct StreamManager {
    streams: Mutex<HashMap<String, StreamHandle>>,
    notification_sender: Mutex<Option<mpsc::Sender<Notification>>>,
}

/// Handle for an active stream
struct StreamHandle {
    join_handle: JoinHandle<()>,
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new() -> Self {
        Self {
            streams: Mutex::new(HashMap::new()),
            notification_sender: Mutex::new(None),
        }
    }

    /// Set the notification sender for streaming responses
    pub fn set_notification_sender(&self, sender: mpsc::Sender<Notification>) {
        let mut notification_sender = self.notification_sender.lock().unwrap();
        *notification_sender = Some(sender);
    }

    /// Register a new stream and get its ID
    pub async fn register_stream(
        &self,
        stream: MpscStream<Result<AgentMessage<ChatResponse>, anyhow::Error>>,
    ) -> String {
        let stream_id = Uuid::new_v4().to_string();

        // Clone the stream ID for the async task
        let stream_id_clone = stream_id.clone();

        // Get a cloned sender if it exists
        let notification_sender = {
            let sender_guard = self.notification_sender.lock().unwrap();
            sender_guard.clone()
        };

        // Before spawning the task, send a direct diagnostic notification to verify the
        // notification pipeline This bypasses the potentially faulty stream
        // processing
        if let Some(sender_ref) = &notification_sender {
            let direct_diagnostic = Notification {
                jsonrpc: Some(Version::V2),
                method: "chat.update".to_string(),
                params: Params::Array(vec![
                    Value::String(stream_id.clone()),
                    json!({ "type": "text", "content": "[DIRECT SYSTEM DIAGNOSTIC] Message sent directly before stream task" }),
                ]),
            };

            // Clone the sender for an immediate task
            let immediate_sender = sender_ref.clone();
            let immediate_stream_id = stream_id.clone();

            // Send the diagnostic notification in an immediate task
            tokio::spawn(async move {
                info!(
                    "Sending direct immediate diagnostic notification for {}",
                    immediate_stream_id
                );
                match immediate_sender.send(direct_diagnostic).await {
                    Ok(_) => info!(
                        "Successfully sent direct diagnostic notification for {}",
                        immediate_stream_id
                    ),
                    Err(e) => error!(
                        "Failed to send direct diagnostic notification for {}: {}",
                        immediate_stream_id, e
                    ),
                }
            });
        }

        info!("About to spawn stream task for {}", stream_id);

        // Create a task to process the stream and send notifications
        // Use spawn_blocking to avoid potential deadlocks in the main runtime
        let join_handle = tokio::task::spawn_blocking(move || {
            info!(
                "Stream task started in blocking context for {}",
                stream_id_clone
            );

            // Create a new runtime for this blocking task
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    error!(
                        "Failed to create runtime for stream {}: {}",
                        stream_id_clone, e
                    );
                    return;
                }
            };

            rt.block_on(async {
                info!(
                    "Stream task running in dedicated runtime for {}",
                    stream_id_clone
                );

                if let Some(sender) = notification_sender {
                    info!(
                        "Notification sender available for {}, proceeding with stream processing",
                        stream_id_clone
                    );
                    Self::process_stream(stream, &stream_id_clone, sender).await;
                } else {
                    error!(
                        "Stream {} processing abandoned due to missing notification sender",
                        stream_id_clone
                    );
                }
            });

            info!("Stream blocking task completed for {}", stream_id_clone);
        });

        info!("Stream task spawned using spawn_blocking for {}", stream_id);

        // Store the stream handle
        let mut streams = self.streams.lock().unwrap();
        streams.insert(stream_id.clone(), StreamHandle { join_handle });

        info!("Registered stream {} and stored handle", stream_id);
        stream_id
    }

    /// Remove a stream by ID
    pub fn remove_stream(&self, id: &str) -> bool {
        let mut streams = self.streams.lock().unwrap();

        if let Some(handle) = streams.remove(id) {
            // Abort the task
            handle.join_handle.abort();
            info!("Removed stream {}", id);
            true
        } else {
            warn!("Attempted to remove non-existent stream {}", id);
            false
        }
    }
    /// Gracefully shuts down all streams
    pub fn shutdown_all_streams(&self) {
        info!("Starting graceful shutdown of all streams");
        let mut streams = self.streams.lock().unwrap();
        let stream_ids: Vec<String> = streams.keys().cloned().collect();
        let count = stream_ids.len();

        for id in stream_ids {
            if let Some(handle) = streams.remove(&id) {
                // Abort the task
                handle.join_handle.abort();
                info!("Shutdown stream {}", id);
            }
        }

        info!("All {} streams shut down", count);

        // Also send a notification indicating server shutdown if possible
        if let Ok(notification_sender) = self.notification_sender.lock() {
            if let Some(sender) = notification_sender.clone() {
                // Try to notify about shutdown, but don't block if it fails
                let _ = std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().ok();
                    if let Some(rt) = rt {
                        rt.block_on(async {
                            let shutdown_notification = jsonrpc_core::Notification {
                                jsonrpc: Some(jsonrpc_core::Version::V2),
                                method: "server.shutdown".to_string(),
                                params: jsonrpc_core::Params::Array(vec![]),
                            };
                            let _ = sender.send(shutdown_notification).await;
                        });
                    }
                });
            }
        }
    }

    /// Process a stream and send notifications
    async fn process_stream(
        mut stream: MpscStream<Result<AgentMessage<ChatResponse>, anyhow::Error>>,
        stream_id: &str,
        notification_sender: mpsc::Sender<Notification>,
    ) {
        // Add a debug notification at stream start
        info!("Process_stream function entered for {}", stream_id);

        let start_notification = Notification {
            jsonrpc: Some(Version::V2),
            method: "chat.update".to_string(),
            params: Params::Array(vec![
                Value::String(stream_id.to_string()),
                json!({ "content": "[Stream Started]", "type": "debug" }),
            ]),
        };

        // Log attempt to send notification
        info!("Creating start notification for stream {}", stream_id);

        // Log the notification for debugging
        if let Ok(notification_str) = serde_json::to_string(&start_notification) {
            info!("Start notification JSON: {}", notification_str);
        }

        info!(
            "Attempting to send start notification for stream {}",
            stream_id
        );

        if let Err(e) = notification_sender.send(start_notification).await {
            error!(
                "Failed to send start notification for stream {}: {}",
                stream_id, e
            );
            return;
        }

        info!(
            "Successfully sent start notification for stream {}",
            stream_id
        );
        info!(
            "Beginning stream loop for {} - waiting for messages",
            stream_id
        );

        // Inject a test message to confirm message delivery is working
        let test_notification = Notification {
            jsonrpc: Some(Version::V2),
            method: "chat.update".to_string(),
            params: Params::Array(vec![
                Value::String(stream_id.to_string()),
                json!({
                    "type": "text",
                    "content": "[DIAGNOSTIC] This is a test message from the server to verify notification delivery."
                }),
            ]),
        };

        info!(
            "Sending diagnostic test notification for stream {}",
            stream_id
        );
        if let Err(e) = notification_sender.send(test_notification).await {
            error!(
                "Failed to send diagnostic test notification for stream {}: {}",
                stream_id, e
            );
        } else {
            info!(
                "Successfully sent diagnostic test notification for stream {}",
                stream_id
            );
        }

        // Count messages for diagnostics
        let mut message_count = 0;

        while let Some(result) = stream.next().await {
            message_count += 1;
            info!("Stream {}: Received message #{}", stream_id, message_count);

            match result {
                Ok(agent_message) => {
                    info!(
                        "Stream {}: Received agent message: {:?}",
                        stream_id, agent_message
                    );

                    // Get the specific type of message
                    let message_type = match &agent_message.message {
                        ChatResponse::Text { text, is_complete } => {
                            info!(
                                "Stream {}: Text message [{}] complete: {}",
                                stream_id, text, is_complete
                            );
                            if let Ok(direct_json) = serde_json::to_value(agent_message.clone()) {
                                // Try direct serialization first
                                info!(
                                    "Stream {}: Direct serialization: {}",
                                    stream_id,
                                    serde_json::to_string(&direct_json).unwrap_or_default()
                                );
                            }
                            "Text"
                        }
                        ChatResponse::ToolCallStart(_) => "ToolCallStart",
                        ChatResponse::ToolCallEnd(_) => "ToolCallEnd",
                        ChatResponse::Usage(_) => "Usage",
                        ChatResponse::Event(_) => "Event",
                    };

                    info!(
                        "Stream {}: Processing message type: {}",
                        stream_id, message_type
                    );

                    // For text messages, ensure we directly pass string content with type field
                    let json_value = if let ChatResponse::Text { text, .. } = &agent_message.message
                    {
                        // Format that matches the test function format exactly
                        json!({
                            "type": "text",
                            "content": text
                        })
                    } else {
                        // For other message types, use normal serialization
                        chat_response_to_json(agent_message.clone())
                    };

                    info!(
                        "Stream {}: Serialized to JSON: {}",
                        stream_id,
                        serde_json::to_string(&json_value).unwrap_or_default()
                    );

                    // Create a notification - make sure we use explicit stream format
                    // that matches what the client is expecting
                    let notification = Notification {
                        jsonrpc: Some(Version::V2),
                        method: "chat.update".to_string(),
                        params: Params::Array(vec![
                            Value::String(stream_id.to_string()),
                            json_value,
                        ]),
                    };

                    // Log the complete notification JSON for debugging
                    if let Ok(notification_str) = serde_json::to_string(&notification) {
                        info!("Notification JSON: {}", notification_str);
                    }

                    // Send the notification and log attempt
                    info!(
                        "Sending notification for stream {} type {}",
                        stream_id, message_type
                    );
                    if let Err(e) = notification_sender.send(notification).await {
                        error!(
                            "Failed to send notification for stream {}: {}",
                            stream_id, e
                        );
                        break;
                    }
                    info!(
                        "Successfully sent notification for stream {} type {}",
                        stream_id, message_type
                    );
                }
                Err(e) => {
                    error!("Stream {} error: {}", stream_id, e);

                    // Create an error notification
                    let notification = Notification {
                        jsonrpc: Some(Version::V2),
                        method: "chat.error".to_string(),
                        params: Params::Array(vec![
                            Value::String(stream_id.to_string()),
                            serde_json::to_value(format!("Stream error: {}", e)).unwrap(),
                        ]),
                    };

                    if let Err(e) = notification_sender.send(notification).await {
                        error!(
                            "Failed to send error notification for stream {}: {}",
                            stream_id, e
                        );
                    }

                    break;
                }
            }
        }

        // Create a completion notification
        let notification = Notification {
            jsonrpc: Some(Version::V2),
            method: "chat.complete".to_string(),
            params: Params::Array(vec![
                Value::String(stream_id.to_string()),
                Value::Bool(true),
            ]),
        };

        // Log the notification for debugging
        if let Ok(notification_str) = serde_json::to_string(&notification) {
            info!("Completion notification: {}", notification_str);
        }

        if let Err(e) = notification_sender.send(notification).await {
            error!(
                "Failed to send completion notification for stream {}: {}",
                stream_id, e
            );
        }

        info!("Stream {} completed processing", stream_id);
    }
}
