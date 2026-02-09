use anyhow::Result;
use forge_app::{AgentRepository, SessionAgentService};
use forge_domain::{
    Agent, AgentId, AppConfigRepository, ProviderId, ProviderRepository, SessionId,
    SessionRepository,
};
use std::sync::Arc;

/// Service for managing session agent switching and retrieval
///
/// Handles agent switching within a session with validation against available
/// agents. Orchestrates infrastructure calls to convert AgentDefinition to
/// runtime Agent with model overrides applied.
pub struct ForgeSessionAgentService<R> {
    infra: Arc<R>,
}

impl<R> ForgeSessionAgentService<R> {
    /// Creates a new ForgeSessionAgentService
    ///
    /// # Arguments
    /// * `infra` - Infrastructure implementing required repository traits
    pub fn new(infra: Arc<R>) -> Self {
        Self { infra }
    }
}

// Implement the SessionAgentService trait
#[async_trait::async_trait]
impl<R: SessionRepository + AgentRepository + AppConfigRepository + ProviderRepository>
    SessionAgentService for ForgeSessionAgentService<R>
{
    async fn switch_agent(
        &self,
        session_id: &SessionId,
        agent_id: &AgentId,
    ) -> Result<()> {
        self.switch_agent(session_id, agent_id).await
    }

    async fn get_session_agent(&self, session_id: &SessionId) -> Result<Agent> {
        self.get_session_agent(session_id).await
    }

    async fn validate_agent_switch(&self, agent_id: &AgentId) -> Result<()> {
        self.validate_agent_switch(agent_id).await
    }

    async fn get_available_agents(&self) -> Result<Vec<Agent>> {
        self.get_available_agents().await
    }
}

impl<R: SessionRepository + AgentRepository + AppConfigRepository + ProviderRepository>
    ForgeSessionAgentService<R>
{
    /// Switches the active agent for a session
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session to modify
    /// * `agent_id` - The ID of the agent to switch to
    ///
    /// # Errors
    /// Returns an error if:
    /// - The session is not found
    /// - The agent is not found or invalid
    /// - The update fails
    pub async fn switch_agent(
        &self,
        session_id: &SessionId,
        agent_id: &AgentId,
    ) -> Result<()> {
        // Validate agent exists
        self.validate_agent_switch(agent_id).await?;

        // Load session state
        let mut state = self
            .infra
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Update agent
        state.agent_id = agent_id.clone();

        // Save session state
        self.infra.save_session(session_id, &state).await?;

        Ok(())
    }

    /// Gets the active agent for a session with any model overrides applied
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session
    ///
    /// # Errors
    /// Returns an error if:
    /// - The session is not found
    /// - The agent is not found
    /// - Infrastructure calls fail
    pub async fn get_session_agent(&self, session_id: &SessionId) -> Result<Agent> {
        // Load session state
        let state = self
            .infra
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Get agent definition from infrastructure
        let agent_defs = self.infra.get_agents().await?;
        let agent_def = agent_defs
            .into_iter()
            .find(|def| def.id == state.agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", state.agent_id))?;

        // Get default provider and model for conversion
        let (default_provider_id, default_model_id) = self.get_defaults().await?;

        // Convert AgentDefinition to runtime Agent
        let mut agent = Agent::from_agent_def(agent_def, default_provider_id, default_model_id);

        // Apply model override from session if set
        if let Some(model_override) = state.model_override {
            agent.model = model_override;
        }

        Ok(agent)
    }

    /// Validates that an agent exists and can be used
    ///
    /// # Arguments
    /// * `agent_id` - The ID of the agent to validate
    ///
    /// # Errors
    /// Returns an error if the agent is not found or invalid
    pub async fn validate_agent_switch(&self, agent_id: &AgentId) -> Result<()> {
        let agent_defs = self.infra.get_agents().await?;
        let exists = agent_defs.iter().any(|def| def.id == *agent_id);

        if !exists {
            anyhow::bail!("Agent not found: {}", agent_id);
        }

        Ok(())
    }

    /// Gets all available agents for mode switching
    ///
    /// # Errors
    /// Returns an error if agents cannot be retrieved
    pub async fn get_available_agents(&self) -> Result<Vec<Agent>> {
        // Get agent definitions
        let agent_defs = self.infra.get_agents().await?;

        // Get default provider and model for conversion
        let (default_provider_id, default_model_id) = self.get_defaults().await?;

        // Convert all definitions to runtime agents
        let agents = agent_defs
            .into_iter()
            .map(|def| Agent::from_agent_def(def, default_provider_id.clone(), default_model_id.clone()))
            .collect();

        Ok(agents)
    }

    /// Helper to get default provider and model from app config
    ///
    /// # Errors
    /// Returns an error if defaults are not configured
    async fn get_defaults(&self) -> Result<(ProviderId, forge_domain::ModelId)> {
        let app_config = self.infra.get_app_config().await?;

        let provider_id = app_config
            .provider
            .ok_or_else(|| anyhow::anyhow!("No default provider configured"))?;

        let model_id = app_config
            .model
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("No default model configured for provider {}", provider_id)
            })?;

        Ok((provider_id, model_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::{AgentDefinition, AppConfig, ConversationId, SessionState};
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;
    use tokio::sync::Mutex;

    // Mock infrastructure for testing
    struct MockInfra {
        sessions: Arc<Mutex<HashMap<SessionId, SessionState>>>,
        agents: Vec<AgentDefinition>,
        app_config: AppConfig,
    }

    impl MockInfra {
        fn new() -> Self {
            let provider_id = ProviderId::from("test-provider".to_string());
            let model_id = forge_domain::ModelId::new("test-model".to_string());

            let mut models = HashMap::new();
            models.insert(provider_id.clone(), model_id);

            Self {
                sessions: Arc::new(Mutex::new(HashMap::new())),
                agents: vec![AgentDefinition::new(AgentId::new("test-agent"))
                    .title("Test Agent".to_string())],
                app_config: AppConfig {
                    provider: Some(provider_id),
                    model: models,
                    ..Default::default()
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl SessionRepository for MockInfra {
        async fn save_session(&self, session_id: &SessionId, state: &SessionState) -> Result<()> {
            self.sessions.lock().await.insert(*session_id, state.clone());
            Ok(())
        }

        async fn load_session(&self, session_id: &SessionId) -> Result<Option<SessionState>> {
            Ok(self.sessions.lock().await.get(session_id).cloned())
        }

        async fn delete_session(&self, session_id: &SessionId) -> Result<()> {
            self.sessions.lock().await.remove(session_id);
            Ok(())
        }

        async fn list_sessions(&self) -> Result<Vec<SessionId>> {
            Ok(self.sessions.lock().await.keys().copied().collect())
        }

        async fn cleanup_expired_sessions(
            &self,
            _ttl: std::time::Duration,
        ) -> Result<usize> {
            Ok(0)
        }
    }

    #[async_trait::async_trait]
    impl AgentRepository for MockInfra {
        async fn get_agents(&self) -> Result<Vec<AgentDefinition>> {
            Ok(self.agents.clone())
        }
    }

    #[async_trait::async_trait]
    impl AppConfigRepository for MockInfra {
        async fn get_app_config(&self) -> Result<AppConfig> {
            Ok(self.app_config.clone())
        }

        async fn set_app_config(&self, _config: &AppConfig) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> Result<Vec<forge_domain::AnyProvider>> {
            Ok(vec![])
        }

        async fn get_provider(
            &self,
            id: ProviderId,
        ) -> Result<forge_domain::ProviderTemplate> {
            use forge_domain::{Provider, Template, URLParameters};
            Ok(Provider {
                id,
                provider_type: Default::default(),
                response: None,
                url: Template::<URLParameters>::new("http://example.com"),
                models: None,
                auth_methods: vec![],
                url_params: vec![],
                credential: None,
            })
        }

        async fn upsert_credential(
            &self,
            _credential: forge_domain::AuthCredential,
        ) -> Result<()> {
            Ok(())
        }

        async fn get_credential(
            &self,
            _id: &ProviderId,
        ) -> Result<Option<forge_domain::AuthCredential>> {
            Ok(None)
        }

        async fn remove_credential(&self, _id: &ProviderId) -> Result<()> {
            Ok(())
        }

        async fn migrate_env_credentials(
            &self,
        ) -> Result<Option<forge_domain::MigrationResult>> {
            Ok(None)
        }
    }

    fn create_test_session(agent_id: AgentId) -> (SessionId, SessionState) {
        let session_id = SessionId::generate();
        let state = SessionState::new(ConversationId::generate(), agent_id);
        (session_id, state)
    }

    #[tokio::test]
    async fn test_switch_agent() {
        let infra = Arc::new(MockInfra::new());
        let fixture = ForgeSessionAgentService::new(infra.clone());

        let agent_id = AgentId::new("test-agent");
        let (session_id, state) = create_test_session(agent_id.clone());

        // Save initial session
        infra.save_session(&session_id, &state).await.unwrap();

        // Switch agent
        let new_agent_id = AgentId::new("test-agent");
        fixture
            .switch_agent(&session_id, &new_agent_id)
            .await
            .unwrap();

        // Verify switch
        let updated_state = infra.load_session(&session_id).await.unwrap().unwrap();
        assert_eq!(updated_state.agent_id, new_agent_id);
    }

    #[tokio::test]
    async fn test_get_session_agent() {
        let infra = Arc::new(MockInfra::new());
        let fixture = ForgeSessionAgentService::new(infra.clone());

        let agent_id = AgentId::new("test-agent");
        let (session_id, state) = create_test_session(agent_id.clone());

        // Save session
        infra.save_session(&session_id, &state).await.unwrap();

        // Get agent
        let actual = fixture.get_session_agent(&session_id).await.unwrap();

        assert_eq!(actual.id, agent_id);
        assert_eq!(actual.title, Some("Test Agent".to_string()));
    }

    #[tokio::test]
    async fn test_get_session_agent_with_model_override() {
        let infra = Arc::new(MockInfra::new());
        let fixture = ForgeSessionAgentService::new(infra.clone());

        let agent_id = AgentId::new("test-agent");
        let (session_id, mut state) = create_test_session(agent_id.clone());

        // Set model override
        let override_model = forge_domain::ModelId::new("override-model".to_string());
        state.model_override = Some(override_model.clone());

        // Save session
        infra.save_session(&session_id, &state).await.unwrap();

        // Get agent
        let actual = fixture.get_session_agent(&session_id).await.unwrap();

        assert_eq!(actual.model, override_model);
    }

    #[tokio::test]
    async fn test_validate_agent_switch_success() {
        let infra = Arc::new(MockInfra::new());
        let fixture = ForgeSessionAgentService::new(infra);

        let agent_id = AgentId::new("test-agent");
        let result = fixture.validate_agent_switch(&agent_id).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_agent_switch_not_found() {
        let infra = Arc::new(MockInfra::new());
        let fixture = ForgeSessionAgentService::new(infra);

        let agent_id = AgentId::new("nonexistent-agent");
        let result = fixture.validate_agent_switch(&agent_id).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Agent not found"));
    }

    #[tokio::test]
    async fn test_get_available_agents() {
        let infra = Arc::new(MockInfra::new());
        let fixture = ForgeSessionAgentService::new(infra);

        let actual = fixture.get_available_agents().await.unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, AgentId::new("test-agent"));
    }
}
