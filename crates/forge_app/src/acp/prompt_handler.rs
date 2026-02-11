//! Prompt execution and streaming for ACP protocol

use agent_client_protocol as acp;
use agent_client_protocol::Client;
use forge_domain::{
    Agent, ChatRequest, ChatResponse, ChatResponseContent, Event, EventValue, InterruptionReason,
};

use crate::{AttachmentService, InterruptionService, Services, SessionAgentService, SessionService};

use super::adapter::AcpAdapter;
use super::conversion;
use super::error::Result;

/// Prompt execution handlers
impl<S: Services> AcpAdapter<S> {
    /// Handles a prompt request from the client
    ///
    /// This is the main method that processes user input and generates responses.
    pub(super) async fn handle_prompt(
        &self,
        arguments: acp::PromptRequest,
    ) -> std::result::Result<acp::PromptResponse, acp::Error> {
        tracing::info!(
            "Received prompt for session: {}, prompt blocks: {}",
            arguments.session_id.0.as_ref(),
            arguments.prompt.len()
        );

        let session_key = arguments.session_id.0.as_ref().to_string();

        // Get domain session ID
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(&session_key)
            .cloned()
            .ok_or_else(|| {
                tracing::error!("Session '{}' not found", session_key);
                acp::Error::invalid_params()
            })?;

        // Get session context (includes cancellation token) from SessionService
        let session_context = self
            .services
            .session_service()
            .get_session_context(&domain_session_id)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        let cancellation_token = session_context.cancellation_token;
        let conversation_id = session_context.state.conversation_id;

        // Convert ACP prompt content to Forge Event
        let mut prompt_text_parts = Vec::new();
        let mut acp_attachments = Vec::new();

        for content_block in &arguments.prompt {
            match content_block {
                acp::ContentBlock::Text(text_content) => {
                    prompt_text_parts.push(text_content.text.clone());
                }
                acp::ContentBlock::ResourceLink(resource_link) => {
                    // IDE sent a resource link - convert URI to @[path] syntax
                    let path = conversion::uri_to_path(&resource_link.uri);
                    prompt_text_parts.push(format!("@[{}]", path));
                }
                acp::ContentBlock::Resource(embedded_resource) => {
                    // IDE sent embedded resource content - convert to Forge attachment
                    match conversion::acp_resource_to_attachment(embedded_resource) {
                        Ok(attachment) => acp_attachments.push(attachment),
                        Err(e) => {
                            tracing::warn!("Failed to convert embedded resource: {}", e);
                        }
                    }
                }
                _ => {
                    // Ignore other content types for now
                }
            }
        }

        let prompt_text = prompt_text_parts.join("\n");

        // Process file tags (@[filename]) from text and ResourceLinks
        let mut attachments = self
            .services
            .attachment_service()
            .attachments(&prompt_text)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        // Add embedded resources from IDE
        attachments.extend(acp_attachments);

        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            value: Some(EventValue::text(prompt_text)),
            timestamp: chrono::Utc::now().to_rfc3339(),
            attachments,
            additional_context: None,
        };

        // Loop to handle interrupts and continuation
        let mut chat_request = ChatRequest::new(event, conversation_id);
        loop {
            // Get the agent for this session with any model override applied
            let agent = self
                .get_session_agent(&session_key)
                .await
                .map_err(acp::Error::from)?;

            tracing::info!(
                "Executing chat for session {} with agent: {}, model: {}",
                session_key,
                agent.id,
                agent.model
            );

            // Flag to track if user wants to continue after an interrupt
            let mut continue_after_interrupt = false;

            // Execute the chat request using SessionOrchestrator
            match self.session_orchestrator.execute_prompt_with_session(&domain_session_id, chat_request).await {
                Ok(mut stream) => {
                    use futures::StreamExt;

                    // Stream responses back to the client as session notifications
                    loop {
                        tokio::select! {
                            // Check for cancellation
                            _ = cancellation_token.cancelled() => {
                                tracing::info!("Session {} cancelled by client", session_key);
                                return Ok(acp::PromptResponse::new(acp::StopReason::Cancelled));
                            }

                            // Process next stream item
                            response_result = stream.next() => {
                                match response_result {
                                    Some(Ok(response)) => {
                                        self.handle_chat_response(&arguments.session_id, response, &mut continue_after_interrupt).await?;
                                    }
                                    Some(Err(e)) => {
                                        tracing::error!("Error in chat stream: {}", e);
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

                    // Check if user wanted to continue after an interrupt
                    if continue_after_interrupt {
                        tracing::info!("Continuing execution after user approved continuation");
                        // Create a new empty event to continue the conversation
                        let continue_event = Event {
                            id: uuid::Uuid::new_v4().to_string(),
                            value: Some(EventValue::text("")),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            attachments: vec![],
                            additional_context: None,
                        };
                        chat_request = ChatRequest::new(continue_event, conversation_id);
                        // Loop back to start a new chat
                        continue;
                    }

                    return Ok(acp::PromptResponse::new(acp::StopReason::EndTurn));
                }
                Err(e) => {
                    tracing::error!("Failed to execute chat: {}", e);
                    return Err(acp::Error::into_internal_error(
                        e.as_ref() as &dyn std::error::Error,
                    ));
                }
            }
        }
    }

    /// Handles a single chat response and converts it to ACP notifications
    async fn handle_chat_response(
        &self,
        session_id: &acp::SessionId,
        response: ChatResponse,
        continue_after_interrupt: &mut bool,
    ) -> std::result::Result<(), acp::Error> {
        // Log what response type we received
        match &response {
            ChatResponse::TaskMessage { .. } => tracing::debug!("Received TaskMessage"),
            ChatResponse::TaskReasoning { .. } => tracing::debug!("Received TaskReasoning"),
            ChatResponse::ToolCallStart(_) => tracing::debug!("Received ToolCallStart"),
            ChatResponse::ToolCallEnd(_) => tracing::debug!("Received ToolCallEnd"),
            ChatResponse::TaskComplete => tracing::debug!("Received TaskComplete"),
            ChatResponse::RetryAttempt { .. } => tracing::debug!("Received RetryAttempt"),
            ChatResponse::Interrupt { .. } => tracing::debug!("Received Interrupt"),
        }

        match response {
            ChatResponse::TaskMessage { content } => {
                self.handle_task_message(session_id, content).await?;
            }
            ChatResponse::TaskReasoning { content } => {
                // Send as agent thought, only if non-empty
                if !content.is_empty() {
                    let notification = acp::SessionNotification::new(
                        session_id.clone(),
                        acp::SessionUpdate::AgentThoughtChunk(acp::ContentChunk::new(
                            acp::ContentBlock::Text(acp::TextContent::new(content)),
                        )),
                    );
                    self.send_notification(notification)
                        .map_err(acp::Error::from)?;
                }
            }
            ChatResponse::ToolCallStart(tool_call) => {
                // Create ACP ToolCall and send as update
                let acp_tool_call = conversion::map_tool_call_to_acp(&tool_call);
                let notification = acp::SessionNotification::new(
                    session_id.clone(),
                    acp::SessionUpdate::ToolCallUpdate(acp_tool_call.into()),
                );
                self.send_notification(notification)
                    .map_err(acp::Error::from)?;
            }
            ChatResponse::ToolCallEnd(tool_result) => {
                // Map tool result to ACP content and send completion update
                let content = conversion::map_tool_output_to_content(&tool_result.output);
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

                let notification =
                    acp::SessionNotification::new(session_id.clone(), acp::SessionUpdate::ToolCallUpdate(update));
                self.send_notification(notification)
                    .map_err(acp::Error::from)?;
            }
            ChatResponse::TaskComplete => {
                // Task is complete, stream will end
            }
            ChatResponse::RetryAttempt { .. } => {
                // Skip retry attempts in ACP output
            }
            ChatResponse::Interrupt { reason } => {
                // Request user permission to continue via ACP standard mechanism
                let should_continue = self
                    .request_continue_permission(session_id, &reason)
                    .await?;

                if !should_continue {
                    // User declined to continue - this will cause the stream to end
                    // and we'll return EndTurn
                    return Ok(());
                }

                // User wants to continue - mark for continuation after stream ends
                *continue_after_interrupt = true;
            }
        }

        Ok(())
    }

    /// Handles task message content
    async fn handle_task_message(
        &self,
        session_id: &acp::SessionId,
        content: ChatResponseContent,
    ) -> std::result::Result<(), acp::Error> {
        match content {
            ChatResponseContent::ToolOutput(_) => {
                // Skip tool outputs in ACP - diffs are shown via ToolCallEnd
            }
            ChatResponseContent::Markdown { text, .. } => {
                // Only send non-empty markdown text
                if !text.is_empty() {
                    let notification = acp::SessionNotification::new(
                        session_id.clone(),
                        acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                            acp::ContentBlock::Text(acp::TextContent::new(text)),
                        )),
                    );
                    self.send_notification(notification)
                        .map_err(acp::Error::from)?;
                }
            }
            ChatResponseContent::ToolInput(title) => {
                // Check if this is a task from an active agent tool call
                let agent_name = title.title.split_whitespace().next().unwrap_or("");
                let is_agent_task = title.title.contains("[Agent]");

                if is_agent_task {
                    // Create a separate tool call for this task that completes immediately
                    let task_desc = if let Some(sub) = &title.sub_title {
                        sub.clone()
                    } else {
                        "Working...".to_string()
                    };

                    // Generate unique ID for this task
                    let task_id = format!("{}-task-{}", agent_name.to_lowercase(), uuid::Uuid::new_v4());

                    // Send ToolCallUpdate for task start with content
                    let start_update = acp::ToolCallUpdate::new(
                        task_id.clone(),
                        acp::ToolCallUpdateFields::new()
                            .kind(acp::ToolKind::Think)
                            .title(agent_name.to_string())
                            .status(acp::ToolCallStatus::InProgress)
                            .content(vec![acp::ToolCallContent::Content(acp::Content::new(
                                acp::ContentBlock::Text(acp::TextContent::new(task_desc.clone())),
                            ))]),
                    );

                    let start_notification = acp::SessionNotification::new(
                        session_id.clone(),
                        acp::SessionUpdate::ToolCallUpdate(start_update),
                    );
                    self.send_notification(start_notification)
                        .map_err(acp::Error::from)?;

                    // Immediately send completion for this task
                    let complete_update = acp::ToolCallUpdate::new(
                        task_id,
                        acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::Completed),
                    );

                    let complete_notification = acp::SessionNotification::new(
                        session_id.clone(),
                        acp::SessionUpdate::ToolCallUpdate(complete_update),
                    );
                    self.send_notification(complete_notification)
                        .map_err(acp::Error::from)?;
                }
            }
        }

        Ok(())
    }

    /// Requests user permission to continue execution after an interruption
    async fn request_continue_permission(
        &self,
        session_id: &acp::SessionId,
        reason: &InterruptionReason,
    ) -> Result<bool> {
        // Get the client connection
        let client_conn = self.client_conn.lock().await;
        let Some(conn) = client_conn.as_ref() else {
            // If no client connection, default to not continuing
            tracing::warn!("No client connection available to request permission");
            return Ok(false);
        };

        // Format interruption message using InterruptionService
        let interruption_service = InterruptionService;
        let message = interruption_service.format_interruption(reason);

        // Create permission options with proper API
        let options = vec![
            acp::PermissionOption::new(
                "continue",
                "Continue Anyway",
                acp::PermissionOptionKind::AllowOnce,
            ),
            acp::PermissionOption::new("stop", "Stop", acp::PermissionOptionKind::RejectOnce),
        ];

        // Create a pseudo tool call for the permission request
        let tool_call_update = acp::ToolCallUpdate::new(
            "interrupt-continue",
            acp::ToolCallUpdateFields::new()
                .status(acp::ToolCallStatus::Pending)
                .title(message.title.clone()),
        );

        // Build and send the permission request
        let mut request = acp::RequestPermissionRequest::new(
            session_id.clone(),
            tool_call_update,
            options.clone(),
        );

        // Add description via meta field
        let mut meta = serde_json::Map::new();
        meta.insert("title".to_string(), serde_json::json!(message.title));
        meta.insert(
            "description".to_string(),
            serde_json::json!(message.description),
        );
        request = request.meta(meta);

        // Send the request and wait for response
        let response = conn
            .request_permission(request)
            .await
            .map_err(|e| super::error::Error::Application(anyhow::anyhow!("Permission request failed: {}", e)))?;

        // Process the response
        match response.outcome {
            acp::RequestPermissionOutcome::Selected(selection) => {
                let should_continue = selection.option_id.0.as_ref() == "continue";
                Ok(should_continue)
            }
            acp::RequestPermissionOutcome::Cancelled => {
                // User cancelled the permission dialog or prompt was cancelled
                Ok(false)
            }
            _ => {
                // Handle any future variants added to the enum
                Ok(false)
            }
        }
    }

    /// Gets the agent for a session and applies any model override
    async fn get_session_agent(&self, session_key: &str) -> Result<Agent> {
        // Get the domain session ID
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(session_key)
            .copied()
            .ok_or_else(|| super::error::Error::Application(anyhow::anyhow!("Session not found")))?;

        // Use SessionAgentService to get the agent with model overrides applied
        self.services
            .session_agent_service()
            .get_session_agent(&domain_session_id)
            .await
            .map_err(|e| super::error::Error::Application(anyhow::anyhow!("{}", e)))
    }
}
