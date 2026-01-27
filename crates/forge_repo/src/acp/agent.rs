//! Forge ACP agent implementation.
//!
//! This module implements the `Agent` trait from the ACP SDK, mapping ACP protocol
//! messages to Forge's existing functionality.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol as acp;
use forge_app::{ConversationService, ForgeApp, Services};
use forge_domain::{AgentId, ChatRequest, ConversationId, Event, EventValue};
use tokio::sync::mpsc;

use crate::acp::{Error, Result};

/// Forge implementation of the ACP Agent trait.
///
/// This struct bridges the ACP protocol with Forge's existing infrastructure,
/// allowing Forge to be invoked as an agent from ACP-compatible IDEs.
pub struct ForgeAgent<S> {
    /// Forge application instance with all services.
    app: Arc<ForgeApp<S>>,
    /// Services for direct access (same as app.services)
    services: Arc<S>,
    /// Channel for sending session notifications to the client.
    session_update_tx: mpsc::UnboundedSender<acp::SessionNotification>,
    /// Counter for generating unique session IDs.
    next_session_id: Cell<u64>,
    /// Mapping from ACP session IDs to Forge conversation IDs.
    session_to_conversation: RefCell<HashMap<String, ConversationId>>,
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
        services: Arc<S>,
        session_update_tx: mpsc::UnboundedSender<acp::SessionNotification>,
    ) -> Self {
        Self {
            app,
            services,
            session_update_tx,
            next_session_id: Cell::new(0),
            session_to_conversation: RefCell::new(HashMap::new()),
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
        let session_key = session_id.0.as_ref().to_string();

        // Check if we already have a mapping
        if let Some(conversation_id) = self.session_to_conversation.borrow().get(&session_key) {
            return Ok(conversation_id.clone());
        }

        // Create a new conversation ID for this session
        let conversation_id = ConversationId::generate();
        self.session_to_conversation
            .borrow_mut()
            .insert(session_key, conversation_id.clone());

        Ok(conversation_id)
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
        tracing::info!(
            "Received initialize request from client: {:?}",
            arguments.client_info
        );

        Ok(acp::InitializeResponse::new(acp::ProtocolVersion::V1)
            .agent_capabilities(acp::AgentCapabilities::new().load_session(true))
            .agent_info(
                acp::Implementation::new(
                    "forge".to_string(),
                    env!("CARGO_PKG_VERSION").to_string(),
                )
                .title("Forge Code".to_string()),
            ))
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
        tracing::info!("Creating new session with cwd: {:?}", arguments.cwd);

        // Generate a new session ID that maps to a Forge conversation ID
        let session_id = self.next_session_id();

        // Create the conversation in Forge's database so it exists when chat() is called
        let conversation_id = self
            .to_conversation_id(&session_id)
            .map_err(|e| acp::Error::from(e))?;

        // Create a new conversation with the generated ID
        let conversation = forge_domain::Conversation::new(conversation_id);

        // Store the conversation using the conversation service
        self.services
            .conversation_service()
            .upsert_conversation(conversation)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        Ok(acp::NewSessionResponse::new(session_id))
    }

    /// Loads an existing session.
    async fn load_session(
        &self,
        arguments: acp::LoadSessionRequest,
    ) -> std::result::Result<acp::LoadSessionResponse, acp::Error> {
        tracing::info!("Loading session: {}", arguments.session_id.0.as_ref());

        // Verify the session exists by attempting to parse it as a conversation ID
        let _conversation_id = self
            .to_conversation_id(&arguments.session_id)
            .map_err(|e| acp::Error::from(e))?;

        Ok(acp::LoadSessionResponse::new())
    }

    /// Handles a prompt request from the client.
    ///
    /// This is the main method that processes user input and generates responses.
    async fn prompt(
        &self,
        arguments: acp::PromptRequest,
    ) -> std::result::Result<acp::PromptResponse, acp::Error> {
        tracing::info!(
            "Received prompt for session: {}, prompt blocks: {}",
            arguments.session_id.0.as_ref(),
            arguments.prompt.len()
        );

        let conversation_id = self
            .to_conversation_id(&arguments.session_id)
            .map_err(|e| acp::Error::from(e))?;

        // Convert ACP prompt content to Forge Event
        let prompt_text = arguments
            .prompt
            .iter()
            .filter_map(|content_block| {
                // Extract text from content blocks
                match content_block {
                    acp::ContentBlock::Text(text_content) => Some(text_content.text.as_str()),
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            value: Some(EventValue::text(prompt_text)),
            timestamp: chrono::Utc::now().to_rfc3339(),
            attachments: Vec::new(),
            additional_context: None,
        };

        let chat_request = ChatRequest::new(event, conversation_id);

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
                                forge_domain::ChatResponse::TaskMessage { content } => {
                                    let text = match content {
                                        forge_domain::ChatResponseContent::PlainText(s) => format!("{s}\n"),
                                        forge_domain::ChatResponseContent::Markdown(s) => format!("{s}\n"),
                                        forge_domain::ChatResponseContent::Title(title) => {
                                            // Format title properly: "Title: subtitle"
                                            if let Some(sub_title) = &title.sub_title {
                                                format!("{}: {}\n", title.title, sub_title)
                                            } else {
                                                format!("{}\n", title.title.clone())
                                            }
                                        }
                                    };
                                    acp::ContentBlock::Text(acp::TextContent::new(text))
                                }
                                forge_domain::ChatResponse::TaskReasoning { content } => {
                                    // Send as agent thought
                                    let notification = acp::SessionNotification::new(
                                        arguments.session_id.clone(),
                                        acp::SessionUpdate::AgentThoughtChunk(
                                            acp::ContentChunk::new(acp::ContentBlock::Text(
                                                acp::TextContent::new(content),
                                            )),
                                        ),
                                    );

                                    self.send_notification(notification)
                                        .map_err(|e| acp::Error::from(e))?;
                                    continue;
                                }
                                forge_domain::ChatResponse::ToolCallStart(_) => {
                                    // For now, skip tool calls in the stream
                                    // TODO: Map to ACP tool calls when supported
                                    continue;
                                }
                                forge_domain::ChatResponse::ToolCallEnd(_) => {
                                    continue;
                                }
                                forge_domain::ChatResponse::TaskComplete => {
                                    // Task is complete, we'll return EndTurn
                                    break;
                                }
                                forge_domain::ChatResponse::RetryAttempt { .. } => {
                                    continue;
                                }
                                forge_domain::ChatResponse::Interrupt { .. } => {
                                    // Interrupted, return cancelled
                                    return Ok(acp::PromptResponse::new(
                                        acp::StopReason::Cancelled,
                                    ));
                                }
                            };

                            let notification = acp::SessionNotification::new(
                                arguments.session_id.clone(),
                                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                                    content,
                                )),
                            );

                            self.send_notification(notification)
                                .map_err(|e| acp::Error::from(e))?;
                        }
                        Err(e) => {
                            tracing::error!("Error in chat stream: {}", e);
                            return Err(acp::Error::into_internal_error(
                                e.as_ref() as &dyn std::error::Error
                            ));
                        }
                    }
                }

                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
            Err(e) => {
                tracing::error!("Failed to execute chat: {}", e);
                Err(acp::Error::into_internal_error(
                    e.as_ref() as &dyn std::error::Error
                ))
            }
        }
    }

    /// Handles cancellation requests.
    async fn cancel(&self, args: acp::CancelNotification) -> std::result::Result<(), acp::Error> {
        tracing::info!(
            "Received cancel request for session: {}",
            args.session_id.0.as_ref()
        );
        // TODO: Implement cancellation logic
        Ok(())
    }

    /// Handles session mode changes.
    async fn set_session_mode(
        &self,
        args: acp::SetSessionModeRequest,
    ) -> std::result::Result<acp::SetSessionModeResponse, acp::Error> {
        tracing::info!("Setting session mode: {:?}", args.mode_id);
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
        let raw_value = serde_json::value::to_raw_value(&serde_json::json!({}))?;
        Ok(acp::ExtResponse::from(Arc::from(raw_value)))
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

        assert_eq!(id1.0.as_ref(), "0");
        assert_eq!(id2.0.as_ref(), "1");
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
