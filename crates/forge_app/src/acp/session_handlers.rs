//! Session lifecycle handlers for ACP protocol

use agent_client_protocol as acp;
use forge_domain::{AgentId, Conversation};

use crate::{AgentRegistry, ConversationService, Services, SessionService};

use super::adapter::AcpAdapter;
use super::state_builders::StateBuilders;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Session lifecycle handlers
impl<S: Services> AcpAdapter<S> {
    /// Handles the initialize request from the client
    ///
    /// This is the first message sent by the client to establish capabilities.
    pub(super) async fn handle_initialize(
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
                            .sse(true), // Support SSE transport
                                        // Stdio is mandatory and always supported
                    ),
            )
            .agent_info(
                acp::Implementation::new("forge".to_string(), VERSION.to_string())
                    .title("Forge Code".to_string()),
            ))
    }

    /// Handles authentication requests
    ///
    /// Currently, Forge doesn't require authentication for local agents.
    pub(super) async fn handle_authenticate(
        &self,
        _arguments: acp::AuthenticateRequest,
    ) -> std::result::Result<acp::AuthenticateResponse, acp::Error> {
        tracing::info!("Received authenticate request");
        Ok(acp::AuthenticateResponse::default())
    }

    /// Creates a new session (conversation in Forge terms)
    pub(super) async fn handle_new_session(
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
            StateBuilders::load_mcp_servers(self.services.as_ref(), &arguments.mcp_servers).await?;
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
        let session_context = self
            .services
            .session_service()
            .get_session_context(&domain_session_id)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        // Create a new conversation with the session's conversation ID
        let conversation = Conversation::new(session_context.state.conversation_id);

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
        let mode_state = StateBuilders::build_session_mode_state(self.services.as_ref(), &active_agent_id)
            .await
            .map_err(acp::Error::from)?;

        // Build session model state with available models
        let model_state = StateBuilders::build_session_model_state(&self.services, &agent)
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

    /// Loads an existing session
    pub(super) async fn handle_load_session(
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
            StateBuilders::load_mcp_servers(self.services.as_ref(), &arguments.mcp_servers).await?;
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
        use crate::SessionAgentService as _;
        let agent = self
            .services
            .session_agent_service()
            .get_session_agent(&domain_session_id)
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;

        let active_agent_id = agent.id.clone();

        // Build session mode state with available agents
        let mode_state = StateBuilders::build_session_mode_state(self.services.as_ref(), &active_agent_id)
            .await
            .map_err(acp::Error::from)?;

        // Build session model state with available models
        let model_state = StateBuilders::build_session_model_state(&self.services, &agent)
            .await
            .map_err(acp::Error::from)?;

        Ok(acp::LoadSessionResponse::new()
            .modes(mode_state)
            .models(model_state))
    }

    /// Handles cancellation requests
    ///
    /// Cancels the active prompt execution for the specified session by
    /// triggering the associated cancellation token.
    pub(super) async fn handle_cancel(
        &self,
        args: acp::CancelNotification,
    ) -> std::result::Result<(), acp::Error> {
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

    /// Handles session mode changes
    ///
    /// Switches the active agent for the session to the specified mode.
    pub(super) async fn handle_set_session_mode(
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
        use crate::SessionAgentService as _;
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

    /// Handles session model changes
    pub(super) async fn handle_set_session_model(
        &self,
        args: acp::SetSessionModelRequest,
    ) -> std::result::Result<acp::SetSessionModelResponse, acp::Error> {
        let session_key = args.session_id.0.as_ref().to_string();
        let model_id = forge_domain::ModelId::new(args.model_id.0.to_string());

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
        use crate::SessionModelService as _;
        self.services
            .session_model_service()
            .set_session_model(&domain_session_id, &model_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to set session model: {}", e);
                acp::Error::into_internal_error(&*e)
            })?;

        // Also update global default (for backward compatibility)
        use crate::AppConfigService as _;
        self.services
            .set_default_model(model_id.clone())
            .await
            .map_err(|e| acp::Error::into_internal_error(&*e))?;
        let _ = self.services.reload_agents().await;

        // Send notification about model change
        tracing::info!("Sending model change notification: {}", model_id);
        let model_update = acp::SessionNotification::new(
            args.session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                acp::ContentBlock::Text(acp::TextContent::new(format!(
                    "Model changed to: {}\n",
                    model_id
                ))),
            )),
        );

        if let Err(e) = self.send_notification(model_update) {
            tracing::warn!("Failed to send a model change notification: {}", e);
        }

        Ok(acp::SetSessionModelResponse::default())
    }
}
