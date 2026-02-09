use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol as acp;
use agent_client_protocol::{Client, SetSessionModelRequest, SetSessionModelResponse};
use forge_app::{
    AgentProviderResolver, AgentRegistry, AppConfigService, AttachmentService, ConversationService,
    ForgeApp, InterruptionService, McpService, ProviderAuthService, ProviderService, Services,
    SessionOrchestrator,
};
use forge_domain::{
    Agent, AgentId, ChatRequest, ConversationId, Event, EventValue, ModelId, SessionId,
};
use tokio::sync::{Mutex, mpsc};

use crate::{Error, Result, VERSION, conversion};

/// Forge implementation of the ACP Agent trait.
pub struct ForgeAgent<S> {
    /// Services for direct access
    services: Arc<S>,
    /// Session orchestrator for coordinating session-related operations
    session_orchestrator: SessionOrchestrator<S>,
    /// Channel for sending session notifications to the client.
    session_update_tx: mpsc::UnboundedSender<acp::SessionNotification>,
    /// Client connection for making RPC calls to the IDE client.
    /// Used for requesting user permission during prompt execution.
    client_conn: Arc<Mutex<Option<Arc<acp::AgentSideConnection>>>>,
    /// Mapping from ACP session IDs to domain session IDs
    /// This is the only session-related state that should remain in the adapter
    acp_to_domain_session: RefCell<HashMap<String, SessionId>>,
}

impl<S: Services> ForgeAgent<S> {
    pub fn new(
        services: Arc<S>,
        session_update_tx: mpsc::UnboundedSender<acp::SessionNotification>,
    ) -> Self {
        let session_orchestrator = SessionOrchestrator::new(services.clone());
        Self {
            services,
            session_orchestrator,
            session_update_tx,
            client_conn: Arc::new(Mutex::new(None)),
            acp_to_domain_session: RefCell::new(HashMap::new()),
        }
    }

    /// Sets the client connection for making RPC calls to the IDE.
    ///
    /// This must be called after creating the agent to enable user interaction
    /// features like requesting permission to continue after failures.
    pub async fn set_client_connection(&self, conn: Arc<acp::AgentSideConnection>) {
        *self.client_conn.lock().await = Some(conn);
    }

    /// Generates a new unique session ID.
    /// Converts an ACP session ID to a Forge conversation ID.
    async fn to_conversation_id(&self, session_id: &acp::SessionId) -> Result<ConversationId> {
        let session_key = session_id.0.as_ref().to_string();

        // Get the domain session ID
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(&session_key)
            .copied()
            .ok_or_else(|| Error::Application(anyhow::anyhow!("Session not found")))?;

        // Get session context from SessionService
        use forge_app::SessionService as _;
        let session_context = self
            .services
            .session_service()
            .get_session_context(&domain_session_id)
            .await
            .map_err(Error::Application)?;

        Ok(session_context.state.conversation_id)
    }

    /// Sends a session notification to the client.
    fn send_notification(&self, notification: acp::SessionNotification) -> Result<()> {
        self.session_update_tx
            .send(notification)
            .map_err(|_| Error::Application(anyhow::anyhow!("Failed to send notification")))
    }

    /// Requests user permission to continue execution after an interruption.
    ///
    /// Uses the ACP `session/request_permission` mechanism to ask the user
    /// if they want to continue after hitting limits (tool failures, chat
    /// turns).
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to request permission for
    /// * `reason` - The interruption reason to display to the user
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the user wants to continue, `Ok(false)` if they
    /// decline, or an error if the request fails.
    async fn request_continue_permission(
        &self,
        session_id: &acp::SessionId,
        reason: &forge_domain::InterruptionReason,
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
        // The ACP protocol requires a tool call context, so we create a synthetic one
        // representing the "continue execution" action
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

        // Add description via meta field since there's no direct description field
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
            .map_err(|e| Error::Application(anyhow::anyhow!("Permission request failed: {}", e)))?;

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

    /// Converts a Forge Agent to an ACP SessionMode.
    /// Builds the SessionModeState from available agents.
    ///
    /// # Errors
    ///
    /// Returns an error if agents cannot be loaded from the registry.
    async fn build_session_mode_state(
        &self,
        current_agent_id: &AgentId,
    ) -> Result<acp::SessionModeState> {
        // Get all available agents from the registry
        let agents = self
            .services
            .agent_registry()
            .get_agents()
            .await
            .map_err(Error::Application)?;

        // Use conversion module to build the state
        Ok(conversion::build_session_mode_state(
            &agents,
            current_agent_id,
        ))
    }

    /// Loads MCP servers from ACP requests into Forge's MCP configuration.
    ///
    /// Converts ACP McpServer types to Forge's McpServerConfig and adds them
    /// to the local MCP configuration without overwriting existing servers.
    ///
    /// # Errors
    ///
    /// Returns an error if MCP servers cannot be loaded or converted.
    async fn load_mcp_servers(&self, mcp_servers: &[acp::McpServer]) -> Result<()> {
        use forge_app::{ExternalMcpServer, McpImportService as _};
        use forge_domain::Scope;

        // Convert ACP MCP servers to ExternalMcpServer format
        let external_servers: Vec<ExternalMcpServer> = mcp_servers
            .iter()
            .map(Self::acp_to_external_mcp_server)
            .collect::<Result<Vec<_>>>()?;

        // Import via McpImportService
        self.services
            .mcp_import_service()
            .import_servers(external_servers, &Scope::Local)
            .await
            .map_err(Error::Application)?;

        // Reload MCP servers to pick up the new configuration
        self.services
            .reload_mcp()
            .await
            .map_err(Error::Application)?;

        Ok(())
    }

    /// Converts an ACP McpServer to ExternalMcpServer format.
    ///
    /// # Errors
    ///
    /// Returns an error if the server configuration is invalid.
    fn acp_to_external_mcp_server(server: &acp::McpServer) -> Result<forge_app::ExternalMcpServer> {
        use forge_app::ExternalMcpServer;

        match server {
            acp::McpServer::Stdio(stdio) => {
                // Convert Vec<EnvVariable> to Vec<(String, String)>
                let env = stdio
                    .env
                    .iter()
                    .map(|e| (e.name.clone(), e.value.clone()))
                    .collect();

                Ok(ExternalMcpServer::Stdio {
                    name: stdio.name.clone(),
                    command: stdio.command.to_string_lossy().to_string(),
                    args: stdio.args.clone(),
                    env,
                })
            }
            acp::McpServer::Http(http) => {
                // Convert Vec<HttpHeader> to Vec<(String, String)>
                let headers = http
                    .headers
                    .iter()
                    .map(|h| (h.name.clone(), h.value.clone()))
                    .collect();

                Ok(ExternalMcpServer::Http {
                    name: http.name.clone(),
                    url: http.url.clone(),
                    headers,
                })
            }
            acp::McpServer::Sse(sse) => {
                // Convert Vec<HttpHeader> to Vec<(String, String)>
                let headers = sse
                    .headers
                    .iter()
                    .map(|h| (h.name.clone(), h.value.clone()))
                    .collect();

                Ok(
                    ExternalMcpServer::Sse {
                        name: sse.name.clone(),
                        url: sse.url.clone(),
                        headers,
                    },
                )
            }
            _ => {
                // Handle future MCP server types that may be added to the protocol
                Err(Error::Application(anyhow::anyhow!(
                    "Unsupported MCP server type"
                )))
            }
        }
    }

    /// Gets the agent for a session and applies any model override.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent cannot be retrieved.
    async fn get_session_agent(&self, session_key: &str) -> Result<Agent> {
        // Get the domain session ID
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(session_key)
            .copied()
            .ok_or_else(|| Error::Application(anyhow::anyhow!("Session not found")))?;

        // Use SessionAgentService to get the agent with model overrides applied
        use forge_app::SessionAgentService as _;
        self.services
            .session_agent_service()
            .get_session_agent(&domain_session_id)
            .await
            .map_err(|e| Error::Application(anyhow::anyhow!("{}", e)))
    }

    /// Builds the SessionModelState from available models for the agent's
    /// provider.
    ///
    /// # Errors
    ///
    /// Returns an error if models cannot be fetched from the provider.
    async fn build_session_model_state(
        &self,
        current_agent: &Agent,
    ) -> Result<acp::SessionModelState> {
        // Resolve the provider for this agent
        let agent_provider_resolver = AgentProviderResolver::new(self.services.clone());
        let provider = agent_provider_resolver
            .get_provider(Some(current_agent.id.clone()))
            .await
            .map_err(Error::Application)?;

        // Refresh provider credentials
        let provider = self
            .services
            .provider_auth_service()
            .refresh_provider_credential(provider)
            .await
            .map_err(Error::Application)?;

        // Fetch models from the provider
        let mut models = self
            .services
            .provider_service()
            .models(provider)
            .await
            .map_err(Error::Application)?;
        models.sort_by(|a, b| a.name.cmp(&b.name));

        // Convert Forge models to ACP ModelInfo
        let available_models: Vec<acp::ModelInfo> = models
            .iter()
            .map(|model| {
                let mut model_info = acp::ModelInfo::new(
                    model.id.to_string(),
                    model.name.clone().unwrap_or_else(|| model.id.to_string()),
                )
                .description(model.description.clone());

                // Add metadata about model capabilities
                let mut meta = serde_json::Map::new();
                if let Some(context_length) = model.context_length {
                    meta.insert(
                        "contextLength".to_string(),
                        serde_json::json!(context_length),
                    );
                }
                if let Some(tools_supported) = model.tools_supported {
                    meta.insert(
                        "toolsSupported".to_string(),
                        serde_json::json!(tools_supported),
                    );
                }
                if let Some(supports_reasoning) = model.supports_reasoning {
                    meta.insert(
                        "supportsReasoning".to_string(),
                        serde_json::json!(supports_reasoning),
                    );
                }
                if !model.input_modalities.is_empty() {
                    let modalities: Vec<String> = model
                        .input_modalities
                        .iter()
                        .map(|m| format!("{:?}", m).to_lowercase())
                        .collect();
                    meta.insert("inputModalities".to_string(), serde_json::json!(modalities));
                }

                if !meta.is_empty() {
                    model_info = model_info.meta(meta);
                }

                model_info
            })
            .collect();

        Ok(
            acp::SessionModelState::new(current_agent.model.to_string(), available_models).meta({
                let mut meta = serde_json::Map::new();
                // Enable search functionality in the model dropdown
                meta.insert("searchable".to_string(), serde_json::json!(true));
                // Show search bar when there are more than 10 models
                meta.insert("searchThreshold".to_string(), serde_json::json!(10));
                // Enable filtering by model capabilities
                meta.insert("filterable".to_string(), serde_json::json!(true));
                // Suggest grouping models by provider
                meta.insert("groupBy".to_string(), serde_json::json!("provider"));
                meta
            }),
        )
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
            .agent_capabilities(
                acp::AgentCapabilities::new()
                    .load_session(true)
                    .mcp_capabilities(
                        acp::McpCapabilities::new()
                            .http(true) // Support HTTP transport
                            .sse(true), /* Support SSE transport
                                         * Stdio is mandatory and always supported */
                    ),
            )
            .agent_info(
                acp::Implementation::new("forge".to_string(), VERSION.to_string())
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

        // Load MCP servers if provided by the client
        if !arguments.mcp_servers.is_empty() {
            tracing::info!(
                "Loading {} MCP servers from client",
                arguments.mcp_servers.len()
            );
            self.load_mcp_servers(&arguments.mcp_servers).await?;
        }

        // Get the active agent or default to forge
        let active_agent_id = self
            .services
            .agent_registry()
            .get_active_agent_id()
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?
            .unwrap_or_default();

        // Create session via SessionService
        // Create session via SessionService
        use forge_app::SessionService as _;
        let domain_session_id = self
            .services
            .session_service()
            .create_session(active_agent_id.clone())
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        // Convert domain session ID to ACP session ID
        let acp_session_id = acp::SessionId::new(domain_session_id.to_string());
        let acp_session_key = acp_session_id.0.as_ref().to_string();

        // Map ACP session ID to domain session ID
        self.acp_to_domain_session
            .borrow_mut()
            .insert(acp_session_key.clone(), domain_session_id);

        // Get session context to retrieve conversation ID
        let session_context: forge_domain::SessionContext = self
            .services
            .session_service()
            .get_session_context(&domain_session_id)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        // Create a new conversation with the session's conversation ID
        let conversation = forge_domain::Conversation::new(session_context.state.conversation_id);

        // Store the conversation using the conversation service
        self.services
            .conversation_service()
            .upsert_conversation(conversation)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        // Get the full agent object to build states
        let agent = self
            .services
            .agent_registry()
            .get_agent(&active_agent_id)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?
            .ok_or_else(|| {
                acp::Error::into_internal_error(&*anyhow::anyhow!(
                    "Agent '{}' not found",
                    active_agent_id
                ))
            })?;

        // Build session mode state with available agents
        let mode_state = self
            .build_session_mode_state(&active_agent_id)
            .await
            .map_err(acp::Error::from)?;

        // Build session model state with available models
        let model_state = self
            .build_session_model_state(&agent)
            .await
            .map_err(acp::Error::from)?;

        tracing::info!(
            "Created session {} with {} models available",
            acp_session_id.0.as_ref(),
            model_state.available_models.len()
        );

        Ok(acp::NewSessionResponse::new(acp_session_id)
            .modes(mode_state)
            .models(model_state))
    }

    /// Loads an existing session.
    async fn load_session(
        &self,
        arguments: acp::LoadSessionRequest,
    ) -> std::result::Result<acp::LoadSessionResponse, acp::Error> {
        tracing::info!("Loading session: {}", arguments.session_id.0.as_ref());

        // Load MCP servers if provided by the client
        if !arguments.mcp_servers.is_empty() {
            tracing::info!(
                "Loading {} MCP servers from client",
                arguments.mcp_servers.len()
            );
            self.load_mcp_servers(&arguments.mcp_servers).await?;
        }

        // Verify the session exists by attempting to get conversation ID
        let _conversation_id = self
            .to_conversation_id(&arguments.session_id)
            .await
            .map_err(acp::Error::from)?;

        // Get the domain session ID
        let session_key = arguments.session_id.0.as_ref().to_string();
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(&session_key)
            .copied()
            .ok_or_else(|| {
                acp::Error::into_internal_error(&*anyhow::anyhow!("Session not found"))
            })?;

        // Get the agent for this session using SessionAgentService
        use forge_app::SessionAgentService as _;
        let agent = self
            .services
            .session_agent_service()
            .get_session_agent(&domain_session_id)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        let active_agent_id = agent.id.clone();

        // Agent is already retrieved above with model overrides applied

        // Build session mode state with available agents
        let mode_state = self
            .build_session_mode_state(&active_agent_id)
            .await
            .map_err(acp::Error::from)?;

        // Build session model state with available models
        let model_state = self
            .build_session_model_state(&agent)
            .await
            .map_err(acp::Error::from)?;

        Ok(acp::LoadSessionResponse::new()
            .modes(mode_state)
            .models(model_state))
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
        use forge_app::SessionService as _;
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
                    // so our attachment service can process it
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

            // Execute the chat request using ForgeApp
            let app = ForgeApp::new(self.services.clone());
            match app.chat(agent.id.clone(), chat_request).await {
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
                                        match response {
                                            forge_domain::ChatResponse::TaskMessage { content } => {
                                                match content {
                                                    forge_domain::ChatResponseContent::ToolOutput(_) => {
                                                        // Skip tool outputs in ACP - diffs are shown via ToolCallEnd
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
                                                                .map_err(acp::Error::from)?;
                                                        }
                                                    }
                                                    forge_domain::ChatResponseContent::ToolInput(title) => {
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
                                                                    .content(vec![acp::ToolCallContent::Content(
                                                                        acp::Content::new(acp::ContentBlock::Text(
                                                                            acp::TextContent::new(task_desc.clone())
                                                                        ))
                                                                    )])
                                                            );

                                                            let start_notification = acp::SessionNotification::new(
                                                                arguments.session_id.clone(),
                                                                acp::SessionUpdate::ToolCallUpdate(start_update),
                                                            );
                                                            self.send_notification(start_notification)
                                                                .map_err(acp::Error::from)?;

                                                            // Immediately send completion for this task
                                                            let complete_update = acp::ToolCallUpdate::new(
                                                                task_id,
                                                                acp::ToolCallUpdateFields::new()
                                                                    .status(acp::ToolCallStatus::Completed)
                                                            );

                                                            let complete_notification = acp::SessionNotification::new(
                                                                arguments.session_id.clone(),
                                                                acp::SessionUpdate::ToolCallUpdate(complete_update),
                                                            );
                                                            self.send_notification(complete_notification)
                                                                .map_err(acp::Error::from)?;
                                                        }
                                                        // If not an agent task, ignore (could be other tool input)
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
                                                let acp_tool_call = conversion::map_tool_call_to_acp(&tool_call);

                                                let notification = acp::SessionNotification::new(
                                                    arguments.session_id.clone(),
                                                    acp::SessionUpdate::ToolCallUpdate(acp_tool_call.into()),
                                                );

                                                self.send_notification(notification)
                                                    .map_err(acp::Error::from)?;
                                            }
                                            forge_domain::ChatResponse::ToolCallEnd(tool_result) => {
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

                                                let notification = acp::SessionNotification::new(
                                                    arguments.session_id.clone(),
                                                    acp::SessionUpdate::ToolCallUpdate(update),
                                                );

                                                self.send_notification(notification)
                                                    .map_err(acp::Error::from)?;
                                            }
                                            forge_domain::ChatResponse::TaskComplete => {
                                                // Task is complete, we'll return EndTurn
                                                break;
                                            }
                                            forge_domain::ChatResponse::RetryAttempt { .. } => {
                                                // Skip retry attempts in ACP output
                                                continue;
                                            }
                                            forge_domain::ChatResponse::Interrupt { reason } => {
                                                // Request user permission to continue via ACP standard mechanism
                                                let should_continue = self.request_continue_permission(&arguments.session_id, &reason).await?;

                                                if !should_continue {
                                                    // User declined to continue - stop execution
                                                    return Ok(acp::PromptResponse::new(
                                                        acp::StopReason::EndTurn,
                                                    ));
                                                }

                                                // User wants to continue - mark for continuation after stream ends
                                                continue_after_interrupt = true;
                                            }
                                        }
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
                        e.as_ref() as &dyn std::error::Error
                    ));
                }
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

        // Get domain session ID
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(&session_key)
            .cloned();

        if let Some(session_id) = domain_session_id {
            // Cancel via SessionService
            use forge_app::SessionService as _;
            self.services
                .session_service()
                .cancel_session(&session_id)
                .await
                .map_err(|e| acp::Error::into_internal_error(&*e))?;

            tracing::info!("Cancelled session: {}", session_key);
        } else {
            tracing::warn!("No active session found: {}", session_key);
        }

        Ok(())
    }

    /// Handles session mode changes.
    ///
    /// Switches the active agent for the session to the specified mode.
    async fn set_session_mode(
        &self,
        args: acp::SetSessionModeRequest,
    ) -> std::result::Result<acp::SetSessionModeResponse, acp::Error> {
        let mode_id = args.mode_id.0.as_ref();
        let session_key = args.session_id.0.as_ref().to_string();

        tracing::info!(
            "Setting session mode for session {} to: {}",
            session_key,
            mode_id
        );

        // Get domain session ID from ACP session ID
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(&session_key)
            .cloned()
            .ok_or_else(|| {
                tracing::error!("Session '{}' not found", session_key);
                acp::Error::invalid_params()
            })?;

        // Parse the mode ID as an agent ID
        let new_agent_id = AgentId::new(mode_id);

        // Switch agent via SessionAgentService
        use forge_app::SessionAgentService as _;
        self.services
            .session_agent_service()
            .switch_agent(&domain_session_id, &new_agent_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to switch agent: {}", e);
                acp::Error::into_internal_error(&*e)
            })?;

        // Send a notification about the mode change
        let mode_update = acp::CurrentModeUpdate::new(acp::SessionModeId::new(mode_id.to_string()));
        let notification = acp::SessionNotification::new(
            args.session_id,
            acp::SessionUpdate::CurrentModeUpdate(mode_update),
        );

        self.send_notification(notification)
            .map_err(acp::Error::from)?;

        Ok(acp::SetSessionModeResponse::new())
    }

    async fn set_session_model(
        &self,
        args: SetSessionModelRequest,
    ) -> agent_client_protocol::Result<SetSessionModelResponse> {
        let session_key = args.session_id.0.as_ref().to_string();
        let model_id = ModelId::new(args.model_id.0.to_string());

        // Get domain session ID from ACP session ID
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(&session_key)
            .cloned()
            .ok_or_else(|| {
                tracing::error!("Session '{}' not found", session_key);
                acp::Error::invalid_params()
            })?;

        // Set model override via SessionModelService
        use forge_app::SessionModelService as _;
        self.services
            .session_model_service()
            .set_session_model(&domain_session_id, &model_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to set session model: {}", e);
                acp::Error::into_internal_error(&*e)
            })?;

        // Also update global default (for backward compatibility)
        self.services.set_default_model(model_id.clone()).await?;
        let _ = self.services.reload_agents().await;

        // Send notification about model change
        let model_update = acp::SessionNotification::new(
            args.session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(acp::ContentBlock::Text(
                acp::TextContent::new(format!("Model changed to: {}", model_id)),
            ))),
        );

        if let Err(e) = self.send_notification(model_update) {
            tracing::warn!("Failed to send a model change notification: {}", e);
        }

        Ok(SetSessionModelResponse::default())
    }
}
