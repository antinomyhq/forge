//! Forge ACP agent implementation.
//!
//! This module implements the `Agent` trait from the ACP SDK, mapping ACP protocol
//! messages to Forge's existing functionality.

use std::cell::Cell;
use std::sync::Arc;

use agent_client_protocol as acp;
use forge_app::{ForgeApp, Services};
use forge_domain::{AgentId, ChatRequest, ConversationId};
use tokio::sync::mpsc;

use crate::{Error, Result};

/// Forge implementation of the ACP Agent trait.
///
/// This struct bridges the ACP protocol with Forge's existing infrastructure,
/// allowing Forge to be invoked as an agent from ACP-compatible IDEs.
pub struct ForgeAgent<S> {
    /// Forge application instance with all services.
    app: Arc<ForgeApp<S>>,
    /// Channel for sending session notifications to the client.
    session_update_tx: mpsc::UnboundedSender<acp::SessionNotification>,
    /// Counter for generating unique session IDs.
    next_session_id: Cell<u64>,
}

impl<S: Services> ForgeAgent<S> {
    /// Creates a new ForgeAgent instance.
    ///
    /// # Arguments
    ///
    /// * `app` - The Forge application instance
    /// * `session_update_tx` - Channel for sending session updates to the client
    pub fn new(
        app: Arc<ForgeApp<S>>,
        session_update_tx: mpsc::UnboundedSender<acp::SessionNotification>,
    ) -> Self {
        Self {
            app,
            session_update_tx,
            next_session_id: Cell::new(0),
        }
    }

    /// Generates a new unique session ID.
    fn next_session_id(&self) -> acp::SessionId {
        let id = self.next_session_id.get();
        self.next_session_id.set(id + 1);
        acp::SessionId::new(id.to_string())
    }

    /// Converts an ACP session ID to a Forge conversation ID.
    fn to_conversation_id(&self, session_id: &acp::SessionId) -> Result<ConversationId> {
        ConversationId::parse(session_id.as_str())
            .map_err(|_| Error::InvalidRequest(format!("Invalid session ID: {}", session_id.as_str())))
    }

    /// Sends a session notification to the client.
    fn send_notification(&self, notification: acp::SessionNotification) -> Result<()> {
        self.session_update_tx
            .send(notification)
            .map_err(|_| Error::Application(anyhow::anyhow!("Failed to send notification")))
    }
}

#[async_trait::async_trait(?Send)]
impl<S: Services> acp::Agent for ForgeAgent<S> {
    /// Handles the initialize request from the client.
    ///
    /// This is the first message sent by the client to establish capabilities.
    async fn initialize(
        &self,
        arguments: acp::InitializeRequest,
    ) -> std::result::Result<acp::InitializeResponse, acp::Error> {
        tracing::info!("Received initialize request from client: {:?}", arguments.client_info);

        Ok(acp::InitializeResponse {
            protocol_version: acp::ProtocolVersion::V1,
            agent_capabilities: acp::AgentCapabilities {
                supports_file_system: true,
                supports_terminal: true,
                supports_tools: true,
                supports_plans: false,
                ..Default::default()
            },
            auth_methods: Vec::new(),
            agent_info: Some(acp::Implementation {
                name: "forge".to_string(),
                title: Some("Forge Code".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            meta: None,
        })
    }

    /// Handles authentication requests.
    ///
    /// Currently, Forge doesn't require authentication for local agents.
    async fn authenticate(
        &self,
        _arguments: acp::AuthenticateRequest,
    ) -> std::result::Result<acp::AuthenticateResponse, acp::Error> {
        tracing::info!("Received authenticate request");
        Ok(acp::AuthenticateResponse::default())
    }

    /// Creates a new session (conversation in Forge terms).
    async fn new_session(
        &self,
        arguments: acp::NewSessionRequest,
    ) -> std::result::Result<acp::NewSessionResponse, acp::Error> {
        tracing::info!("Creating new session with options: {:?}", arguments.options);

        // Generate a new session ID that maps to a Forge conversation ID
        let session_id = self.next_session_id();

        Ok(acp::NewSessionResponse {
            session_id,
            modes: None,
            meta: None,
        })
    }

    /// Loads an existing session.
    async fn load_session(
        &self,
        arguments: acp::LoadSessionRequest,
    ) -> std::result::Result<acp::LoadSessionResponse, acp::Error> {
        tracing::info!("Loading session: {}", arguments.session_id.as_str());

        // Verify the session exists by attempting to parse it as a conversation ID
        let _conversation_id = self
            .to_conversation_id(&arguments.session_id)
            .map_err(|e| acp::Error::from(e))?;

        Ok(acp::LoadSessionResponse {
            modes: None,
            meta: None,
        })
    }

    /// Handles a prompt request from the client.
    ///
    /// This is the main method that processes user input and generates responses.
    async fn prompt(
        &self,
        arguments: acp::PromptRequest,
    ) -> std::result::Result<acp::PromptResponse, acp::Error> {
        tracing::info!("Received prompt for session: {}", arguments.session_id.as_str());

        let conversation_id = self
            .to_conversation_id(&arguments.session_id)
            .map_err(|e| acp::Error::from(e))?;

        // Convert ACP prompt content to Forge chat request
        let prompt_text = arguments
            .prompt
            .iter()
            .filter_map(|content| {
                if let acp::Content::Text(text) = content {
                    Some(text.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let chat_request = ChatRequest {
            conversation_id,
            prompt: prompt_text,
            ..Default::default()
        };

        // Get the default agent ID (or could be configured)
        let agent_id = AgentId::default();

        // Execute the chat request
        match self.app.chat(agent_id, chat_request).await {
            Ok(mut stream) => {
                use futures::StreamExt;

                // Stream responses back to the client as session notifications
                while let Some(response_result) = stream.next().await {
                    match response_result {
                        Ok(response) => {
                            // Convert ChatResponse to ACP session notification
                            let content = match response {
                                forge_domain::ChatResponse::Content(content) => {
                                    acp::Content::Text(acp::TextContent {
                                        text: content.into(),
                                        meta: None,
                                    })
                                }
                                forge_domain::ChatResponse::ToolCall(_) => {
                                    // For now, skip tool calls in the stream
                                    // TODO: Map to ACP tool calls when supported
                                    continue;
                                }
                                forge_domain::ChatResponse::Error(err) => {
                                    tracing::error!("Error in chat stream: {}", err);
                                    return Err(acp::Error::internal_error_with_data(err));
                                }
                                _ => continue,
                            };

                            let notification = acp::SessionNotification {
                                session_id: arguments.session_id.clone(),
                                update: acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk {
                                    content,
                                    meta: None,
                                }),
                                meta: None,
                            };

                            self.send_notification(notification)
                                .map_err(|e| acp::Error::from(e))?;
                        }
                        Err(e) => {
                            tracing::error!("Error in chat stream: {}", e);
                            return Err(acp::Error::internal_error_with_data(e.to_string()));
                        }
                    }
                }

                Ok(acp::PromptResponse {
                    stop_reason: acp::StopReason::EndTurn,
                    meta: None,
                })
            }
            Err(e) => {
                tracing::error!("Failed to execute chat: {}", e);
                Err(acp::Error::internal_error_with_data(e.to_string()))
            }
        }
    }

    /// Handles cancellation requests.
    async fn cancel(&self, args: acp::CancelNotification) -> std::result::Result<(), acp::Error> {
        tracing::info!("Received cancel request for session: {}", args.session_id.as_str());
        // TODO: Implement cancellation logic
        Ok(())
    }

    /// Handles session mode changes.
    async fn set_session_mode(
        &self,
        args: acp::SetSessionModeRequest,
    ) -> std::result::Result<acp::SetSessionModeResponse, acp::Error> {
        tracing::info!("Setting session mode: {:?}", args.mode);
        Ok(acp::SetSessionModeResponse::default())
    }

    /// Handles extension method calls.
    async fn ext_method(
        &self,
        args: acp::ExtRequest,
    ) -> std::result::Result<acp::ExtResponse, acp::Error> {
        tracing::info!(
            "Received extension method call: method={}, params={:?}",
            args.method,
            args.params
        );
        // Return empty response for now
        Ok(serde_json::value::to_raw_value(&serde_json::json!({}))?.into())
    }

    /// Handles extension notifications.
    async fn ext_notification(
        &self,
        args: acp::ExtNotification,
    ) -> std::result::Result<(), acp::Error> {
        tracing::info!(
            "Received extension notification: method={}, params={:?}",
            args.method,
            args.params
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_session_id_generation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let app = Arc::new(ForgeApp::new(Arc::new(forge_test_kit::MockServices::new())));
        let agent = ForgeAgent::new(app, tx);

        let id1 = agent.next_session_id();
        let id2 = agent.next_session_id();

        assert_eq!(id1.as_str(), "0");
        assert_eq!(id2.as_str(), "1");
    }

    #[test]
    fn test_conversation_id_conversion() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let app = Arc::new(ForgeApp::new(Arc::new(forge_test_kit::MockServices::new())));
        let agent = ForgeAgent::new(app, tx);

        let valid_uuid = "550e8400-e29b-41d4-a716-446655440000";
        let session_id = acp::SessionId::new(valid_uuid.to_string());

        let result = agent.to_conversation_id(&session_id);
        assert!(result.is_ok());

        let invalid_session_id = acp::SessionId::new("invalid".to_string());
        let result = agent.to_conversation_id(&invalid_session_id);
        assert!(result.is_err());
    }
}
