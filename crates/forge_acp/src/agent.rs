//! Forge ACP agent implementation.
//!
//! This module implements the `Agent` trait from the ACP SDK, mapping ACP
//! protocol messages to Forge's existing functionality.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol as acp;
use forge_app::{ConversationService, ForgeApp, Services};
use forge_domain::{
    AgentId, ChatRequest, ConversationId, Event, EventValue, ToolCallFull, ToolName, ToolValue,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{Error, Result};

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
    /// Cancellation tokens for active sessions.
    /// Allows clients to interrupt long-running operations.
    cancellation_tokens: RefCell<HashMap<String, CancellationToken>>,
}

impl<S: Services> ForgeAgent<S> {
    /// Creates a new ForgeAgent instance.
    ///
    /// # Arguments
    ///
    /// * `app` - The Forge application instance
    /// * `session_update_tx` - Channel for sending session updates to the
    ///   client
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
            cancellation_tokens: RefCell::new(HashMap::new()),
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
            return Ok(*conversation_id);
        }

        // Create a new conversation ID for this session
        let conversation_id = ConversationId::generate();
        self.session_to_conversation
            .borrow_mut()
            .insert(session_key, conversation_id);

        Ok(conversation_id)
    }

    /// Sends a session notification to the client.
    fn send_notification(&self, notification: acp::SessionNotification) -> Result<()> {
        self.session_update_tx
            .send(notification)
            .map_err(|_| Error::Application(anyhow::anyhow!("Failed to send notification")))
    }

    /// Maps a Forge tool name to an ACP ToolKind.
    fn map_tool_kind(tool_name: &ToolName) -> acp::ToolKind {
        match tool_name.as_str() {
            "read" => acp::ToolKind::Read,
            "write" | "patch" => acp::ToolKind::Edit,
            "remove" | "undo" => acp::ToolKind::Delete,
            "fs_search" | "sem_search" => acp::ToolKind::Search,
            "shell" => acp::ToolKind::Execute,
            "fetch" => acp::ToolKind::Fetch,
            "sage" => acp::ToolKind::Think, // Research agent
            _ => {
                // Check MCP tool patterns
                let name = tool_name.as_str();
                if name.starts_with("mcp_") {
                    if name.contains("read") || name.contains("get") || name.contains("fetch") {
                        acp::ToolKind::Read
                    } else if name.contains("search")
                        || name.contains("query")
                        || name.contains("find")
                    {
                        acp::ToolKind::Search
                    } else if name.contains("write")
                        || name.contains("update")
                        || name.contains("create")
                    {
                        acp::ToolKind::Edit
                    } else if name.contains("delete") || name.contains("remove") {
                        acp::ToolKind::Delete
                    } else if name.contains("execute") || name.contains("run") {
                        acp::ToolKind::Execute
                    } else {
                        acp::ToolKind::Other
                    }
                } else {
                    acp::ToolKind::Other
                }
            }
        }
    }

    /// Extracts file locations from tool arguments for "follow-along" features.
    fn extract_file_locations(
        tool_name: &ToolName,
        arguments: &serde_json::Value,
    ) -> Vec<acp::ToolCallLocation> {
        match tool_name.as_str() {
            "read" | "write" | "patch" | "remove" | "undo" => {
                if let Some(file_path) = arguments.get("file_path").and_then(|v| v.as_str()) {
                    vec![acp::ToolCallLocation::new(PathBuf::from(file_path))]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    /// Maps a Forge ToolCallFull to an ACP ToolCall.
    fn map_tool_call_to_acp(tool_call: &ToolCallFull) -> acp::ToolCall {
        let tool_call_id = tool_call
            .call_id
            .as_ref()
            .map(|id| id.as_str().to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let title = format!("{}", tool_call.name.as_str());
        let kind = Self::map_tool_kind(&tool_call.name);
        let locations = Self::extract_file_locations(
            &tool_call.name,
            &serde_json::to_value(&tool_call.arguments).unwrap_or(serde_json::json!({})),
        );

        acp::ToolCall::new(tool_call_id, title)
            .kind(kind)
            .status(acp::ToolCallStatus::Pending)
            .locations(locations)
            .raw_input(
                serde_json::to_value(&tool_call.arguments)
                    .ok()
                    .filter(|v| !v.is_null()),
            )
    }

    /// Maps a Forge ToolOutput to ACP ToolCallContent.
    fn map_tool_output_to_content(output: &forge_domain::ToolOutput) -> Vec<acp::ToolCallContent> {
        output
            .values
            .iter()
            .filter_map(|value| match value {
                ToolValue::Text(text) => {
                    Some(acp::ToolCallContent::Content(acp::Content::new(
                        acp::ContentBlock::Text(acp::TextContent::new(text.clone())),
                    )))
                }
                ToolValue::Image(image) => Some(acp::ToolCallContent::Content(acp::Content::new(
                    acp::ContentBlock::Image(acp::ImageContent::new(
                        image.data(),
                        image.mime_type(),
                    )),
                ))),
                ToolValue::AI { value, .. } => {
                    Some(acp::ToolCallContent::Content(acp::Content::new(
                        acp::ContentBlock::Text(acp::TextContent::new(value.clone())),
                    )))
                }
                ToolValue::Empty => None,
            })
            .collect()
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

        // Create the conversation in Forge's database so it exists when chat() is
        // called
        let conversation_id = self
            .to_conversation_id(&session_id)
            .map_err(acp::Error::from)?;

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
            .map_err(acp::Error::from)?;

        Ok(acp::LoadSessionResponse::new())
    }

    /// Handles a prompt request from the client.
    ///
    /// This is the main method that processes user input and generates
    /// responses.
    async fn prompt(
        &self,
        arguments: acp::PromptRequest,
    ) -> std::result::Result<acp::PromptResponse, acp::Error> {
        tracing::info!(
            "Received prompt for session: {}, prompt blocks: {}",
            arguments.session_id.0.as_ref(),
            arguments.prompt.len()
        );

        let session_key = arguments.session_id.0.as_ref().to_string();

        // Create a new cancellation token for this prompt
        let cancellation_token = CancellationToken::new();
        self.cancellation_tokens
            .borrow_mut()
            .insert(session_key.clone(), cancellation_token.clone());

        let conversation_id = self
            .to_conversation_id(&arguments.session_id)
            .map_err(acp::Error::from)?;

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
                loop {
                    tokio::select! {
                        // Check for cancellation
                        _ = cancellation_token.cancelled() => {
                            tracing::info!("Session {} cancelled by client", session_key);
                            
                            // Clean up the cancellation token
                            self.cancellation_tokens.borrow_mut().remove(&session_key);
                            
                            return Ok(acp::PromptResponse::new(acp::StopReason::Cancelled));
                        }
                        
                        // Process next stream item
                        response_result = stream.next() => {
                            match response_result {
                                Some(Ok(response)) => {
                            match response {
                                forge_domain::ChatResponse::TaskMessage { content } => {
                                    match content {
                                        forge_domain::ChatResponseContent::ToolOutput(_) => {
                                            // Skip tool outputs in ACP - they're too verbose
                                            continue;
                                        }
                                        forge_domain::ChatResponseContent::Markdown {
                                            text,
                                            ..
                                        } => {
                                            // Only send non-empty markdown text
                                            if !text.is_empty() {
                                                let notification = acp::SessionNotification::new(
                                                    arguments.session_id.clone(),
                                                    acp::SessionUpdate::AgentMessageChunk(
                                                        acp::ContentChunk::new(
                                                            acp::ContentBlock::Text(
                                                                acp::TextContent::new(text),
                                                            ),
                                                        ),
                                                    ),
                                                );
                                                self.send_notification(notification)
                                                    .map_err(|e| acp::Error::from(e))?;
                                            }
                                        }
                                        forge_domain::ChatResponseContent::ToolInput(_) => {
                                            // Skip tool input notifications - too verbose for ACP
                                            continue;
                                        }
                                    }
                                }
                                forge_domain::ChatResponse::TaskReasoning { content } => {
                                    // Send as agent thought, only if non-empty
                                    if !content.is_empty() {
                                        let notification = acp::SessionNotification::new(
                                            arguments.session_id.clone(),
                                            acp::SessionUpdate::AgentThoughtChunk(
                                                acp::ContentChunk::new(acp::ContentBlock::Text(
                                                    acp::TextContent::new(content),
                                                )),
                                            ),
                                        );

                                        self.send_notification(notification)
                                            .map_err(acp::Error::from)?;
                                    }
                                }
                                forge_domain::ChatResponse::ToolCallStart(tool_call) => {
                                    // Create ACP ToolCall and send as update
                                    let acp_tool_call = Self::map_tool_call_to_acp(&tool_call);

                                    let notification = acp::SessionNotification::new(
                                        arguments.session_id.clone(),
                                        acp::SessionUpdate::ToolCallUpdate(acp_tool_call.into()),
                                    );

                                    self.send_notification(notification)
                                        .map_err(|e| acp::Error::from(e))?;
                                }
                                forge_domain::ChatResponse::ToolCallEnd(tool_result) => {
                                    // Map tool result to ACP content and send completion update
                                    let content = Self::map_tool_output_to_content(&tool_result.output);
                                    let status = if tool_result.output.is_error {
                                        acp::ToolCallStatus::Failed
                                    } else {
                                        acp::ToolCallStatus::Completed
                                    };

                                    let tool_call_id = tool_result
                                        .call_id
                                        .as_ref()
                                        .map(|id| id.as_str().to_string())
                                        .unwrap_or_else(|| "unknown".to_string());

                                    let update = acp::ToolCallUpdate::new(
                                        tool_call_id,
                                        acp::ToolCallUpdateFields::new()
                                            .status(status)
                                            .content(content),
                                    );

                                    let notification = acp::SessionNotification::new(
                                        arguments.session_id.clone(),
                                        acp::SessionUpdate::ToolCallUpdate(update),
                                    );

                                    self.send_notification(notification)
                                        .map_err(|e| acp::Error::from(e))?;
                                }
                                forge_domain::ChatResponse::TaskComplete => {
                                    // Task is complete, we'll return EndTurn
                                    break;
                                }
                                forge_domain::ChatResponse::RetryAttempt { .. } => {
                                    // Skip retry attempts in ACP output
                                    continue;
                                }
                                forge_domain::ChatResponse::Interrupt { .. } => {
                                    // Interrupted, return cancelled
                                    // Clean up cancellation token
                                    self.cancellation_tokens.borrow_mut().remove(&session_key);
                                    
                                    return Ok(acp::PromptResponse::new(
                                        acp::StopReason::Cancelled,
                                    ));
                                }
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!("Error in chat stream: {}", e);
                            
                            // Clean up cancellation token
                            self.cancellation_tokens.borrow_mut().remove(&session_key);
                            
                            return Err(acp::Error::into_internal_error(
                                e.as_ref() as &dyn std::error::Error
                            ));
                        }
                        None => {
                            // Stream ended normally
                            break;
                        }
                    }
                }
            }
        }

                // Clean up cancellation token
                self.cancellation_tokens.borrow_mut().remove(&session_key);

                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
            Err(e) => {
                tracing::error!("Failed to execute chat: {}", e);
                
                // Clean up cancellation token
                self.cancellation_tokens.borrow_mut().remove(&session_key);
                
                Err(acp::Error::into_internal_error(
                    e.as_ref() as &dyn std::error::Error
                ))
            }
        }
    }

    /// Handles cancellation requests.
    ///
    /// Cancels the active prompt execution for the specified session by
    /// triggering the associated cancellation token.
    async fn cancel(&self, args: acp::CancelNotification) -> std::result::Result<(), acp::Error> {
        let session_key = args.session_id.0.as_ref().to_string();
        
        tracing::info!("Received cancel request for session: {}", session_key);

        // Trigger the cancellation token if it exists
        if let Some(token) = self.cancellation_tokens.borrow().get(&session_key) {
            token.cancel();
            tracing::info!("Cancelled session: {}", session_key);
        } else {
            tracing::warn!("No active prompt for session: {}", session_key);
        }

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
