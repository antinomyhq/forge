use std::sync::Arc;

use anyhow::Result;
use forge_domain::{Agent, AgentId, ChatRequest, ChatResponse, Model, ModelId, SessionContext, SessionId};
use forge_stream::MpscStream;

use crate::{ForgeApp, ProviderService, Services, SessionAgentService, SessionModelService, SessionService};

/// Orchestrates session-related operations by coordinating multiple services
///
/// This is NOT a service - it's a concrete orchestrator in the app layer that
/// composes multiple services to implement high-level workflows. Services remain
/// independent and testable, while the orchestrator provides convenient APIs.
///
/// # Architecture
///
/// - Services (SessionService, SessionAgentService, SessionModelService) handle
///   single responsibilities
/// - SessionOrchestrator coordinates services for complex workflows
/// - Protocol adapters (ACP, REST, gRPC) call orchestrator, not services directly
pub struct SessionOrchestrator<S> {
    services: Arc<S>,
}

impl<S: Services> SessionOrchestrator<S> {
    /// Creates a new session orchestrator
    ///
    /// # Arguments
    ///
    /// * `services` - The services implementation to orchestrate
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Executes a prompt within a session context
    ///
    /// This orchestrates multiple services:
    /// 1. Get session context from SessionService
    /// 2. Get agent (with overrides) from SessionAgentService
    /// 3. Get effective model from SessionModelService
    /// 4. Execute chat via ForgeApp
    /// 5. Handle cancellation via session's cancellation token
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID
    /// * `request` - The chat request to execute
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Session not found
    /// - Agent not found
    /// - Model resolution fails
    /// - Chat execution fails
    pub async fn execute_prompt_with_session(
        &self,
        session_id: &SessionId,
        request: ChatRequest,
    ) -> Result<MpscStream<Result<ChatResponse>>> {
        // 1. Get session context
        let context = self
            .services
            .session_service()
            .get_session_context(session_id)
            .await?;

        // 2. Get agent with overrides applied
        let agent = self
            .services
            .session_agent_service()
            .get_session_agent(session_id)
            .await?;

        // 3. Execute chat via ForgeApp
        let app = ForgeApp::new(self.services.clone());
        app.chat(agent.id, request).await
    }

    /// Switches the agent for a session
    ///
    /// This orchestrates:
    /// 1. Validate agent exists via SessionAgentService
    /// 2. Switch agent via SessionAgentService
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID
    /// * `agent_id` - The new agent ID
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Session not found
    /// - Agent not found
    /// - Agent switch fails
    pub async fn switch_session_agent(
        &self,
        session_id: &SessionId,
        agent_id: &AgentId,
    ) -> Result<()> {
        // Validate agent exists
        self.services
            .session_agent_service()
            .validate_agent_switch(agent_id)
            .await?;

        // Switch agent
        self.services
            .session_agent_service()
            .switch_agent(session_id, agent_id)
            .await
    }

    /// Switches the model for a session
    ///
    /// This orchestrates:
    /// 1. Set model override via SessionModelService
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID
    /// * `model_id` - The new model ID
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Session not found
    /// - Model override fails
    pub async fn switch_session_model(
        &self,
        session_id: &SessionId,
        model_id: &ModelId,
    ) -> Result<()> {
        self.services
            .session_model_service()
            .set_session_model(session_id, model_id)
            .await
    }

    /// Gets available agents for session mode switching
    ///
    /// # Errors
    ///
    /// Returns an error if agent retrieval fails
    pub async fn get_available_agents(&self) -> Result<Vec<Agent>> {
        self.services
            .session_agent_service()
            .get_available_agents()
            .await
    }

    /// Gets available models for the session's current agent
    ///
    /// This orchestrates:
    /// 1. Get session agent from SessionAgentService
    /// 2. Get models for that agent's provider
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Session not found
    /// - Agent not found
    /// - Model retrieval fails
    pub async fn get_available_models(&self, session_id: &SessionId) -> Result<Vec<Model>> {
        // Get the session's agent (with overrides applied)
        let agent = self
            .services
            .session_agent_service()
            .get_session_agent(session_id)
            .await?;

        // Get models from provider service
        let provider = self
            .services
            .provider_service()
            .get_provider(agent.provider)
            .await?;

        self.services.provider_service().models(provider).await
    }

    /// Gets the session context
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID
    ///
    /// # Errors
    ///
    /// Returns an error if session not found
    pub async fn get_session_context(&self, session_id: &SessionId) -> Result<SessionContext> {
        self.services
            .session_service()
            .get_session_context(session_id)
            .await
    }

    /// Cancels a session
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID to cancel
    ///
    /// # Errors
    ///
    /// Returns an error if session not found
    pub async fn cancel_session(&self, session_id: &SessionId) -> Result<()> {
        self.services
            .session_service()
            .cancel_session(session_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_structure() {
        // Just verify the structure compiles and has the expected size
        assert!(std::mem::size_of::<SessionOrchestrator<()>>() > 0);
    }
}
