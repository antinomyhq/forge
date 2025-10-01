use anyhow::Result;
use async_trait::async_trait;
use forge_domain::{AgentId, ModelId};
use thiserror::Error;

// Import the service traits we need
use crate::services::{AgentLoaderService, AppConfigService};

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Agent '{0}' is not available")]
    AgentNotAvailable(AgentId),
    #[error("Model '{0}' is not available")]
    ModelNotAvailable(ModelId),
    #[error("Agent '{agent}' is not compatible with model '{model}'")]
    IncompatibleAgentModel { agent: AgentId, model: ModelId },
    #[error("Configuration validation failed: {0}")]
    ValidationFailed(String),
    #[error("Configuration not found")]
    NotFound,
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Service error: {0}")]
    ServiceError(#[from] anyhow::Error),
}

/// Configuration source with precedence ordering
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigSource {
    Environment = 3, // Highest precedence
    AppConfig = 2,   // Medium precedence
    Default = 1,     // Lowest precedence
}

/// Resolved configuration with source tracking
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub agent: Option<(AgentId, ConfigSource)>,
    pub model: Option<(ModelId, ConfigSource)>,
}

#[async_trait]
pub trait ConfigurationResolver: Send + Sync {
    /// Resolve the operating agent with proper precedence
    async fn resolve_agent(&self) -> Result<Option<(AgentId, ConfigSource)>, ConfigError>;

    /// Resolve the operating model with proper precedence  
    async fn resolve_model(&self) -> Result<Option<(ModelId, ConfigSource)>, ConfigError>;

    /// Resolve both agent and model atomically
    async fn resolve_config(&self) -> Result<ResolvedConfig, ConfigError>;

    /// Validate that the resolved configuration is valid
    async fn validate_resolved_config(&self, config: &ResolvedConfig) -> Result<(), ConfigError>;

    /// Get the current app config
    async fn get_app_config(&self) -> Result<Option<crate::dto::AppConfig>, ConfigError>;

    /// Update the app config
    async fn set_app_config(&self, config: &crate::dto::AppConfig) -> Result<(), ConfigError>;
}

/// Default implementation of ConfigurationResolver
pub struct DefaultConfigurationResolver<S: crate::Services> {
    services: S,
    environment: std::collections::BTreeMap<String, String>,
}

impl<S: crate::Services> DefaultConfigurationResolver<S> {
    pub fn new(services: S, environment: Vec<String>) -> Self {
        Self { services, environment: parse_env(environment) }
    }

    fn get_agent_from_env(&self) -> Option<AgentId> {
        self.environment
            .get("FORGE_AGENT")
            .map(|agent| AgentId::new(agent.clone()))
    }

    fn get_model_from_env(&self) -> Option<ModelId> {
        self.environment
            .get("FORGE_MODEL")
            .map(|model| ModelId::new(model.clone()))
    }
}

#[async_trait]
impl<S: crate::Services> ConfigurationResolver for DefaultConfigurationResolver<S> {
    async fn resolve_agent(&self) -> Result<Option<(AgentId, ConfigSource)>, ConfigError> {
        // Environment variables have highest precedence
        if let Some(agent) = self.get_agent_from_env() {
            return Ok(Some((agent, ConfigSource::Environment)));
        }

        // Then app config
        if let Some(config) = self.services.get_app_config().await
            && let Some(agent) = config.operating_agent
            && !agent.as_str().is_empty()
        {
            return Ok(Some((agent, ConfigSource::AppConfig)));
        }

        // Finally default
        Ok(Some((AgentId::default(), ConfigSource::Default)))
    }

    async fn resolve_model(&self) -> Result<Option<(ModelId, ConfigSource)>, ConfigError> {
        // Environment variables have highest precedence
        if let Some(model) = self.get_model_from_env() {
            return Ok(Some((model, ConfigSource::Environment)));
        }

        // Then app config
        if let Some(config) = self.services.get_app_config().await
            && let Some(model) = config.operating_model
            && !model.as_str().is_empty()
        {
            return Ok(Some((model, ConfigSource::AppConfig)));
        }

        // No default model - user must select one
        Ok(None)
    }

    async fn resolve_config(&self) -> Result<ResolvedConfig, ConfigError> {
        let agent = self.resolve_agent().await?;
        let model = self.resolve_model().await?;

        Ok(ResolvedConfig { agent, model })
    }

    async fn validate_resolved_config(&self, config: &ResolvedConfig) -> Result<(), ConfigError> {
        // Validate that the agent exists if specified
        if let Some((agent, _)) = &config.agent {
            let agents = self.services.get_agents().await?;
            if !agents.iter().any(|a| a.id == *agent) {
                return Err(ConfigError::AgentNotAvailable(agent.clone()));
            }
        }

        // TODO: Validate model availability when we have a proper way to access
        // provider For now, we'll skip model validation to avoid provider
        // service complexity

        // Validate model-agent compatibility if both are specified
        if let (Some((agent, _)), Some((model, _))) = (&config.agent, &config.model)
            && let Some(incompatibility) =
                self.check_agent_model_compatibility(agent, model).await?
        {
            return Err(incompatibility);
        }

        Ok(())
    }

    async fn get_app_config(&self) -> Result<Option<crate::dto::AppConfig>, ConfigError> {
        Ok(self.services.get_app_config().await)
    }

    async fn set_app_config(&self, config: &crate::dto::AppConfig) -> Result<(), ConfigError> {
        // Validate the configuration before setting
        if let Err(e) = config.validate() {
            return Err(ConfigError::ValidationFailed(e.to_string()));
        }

        self.services.set_app_config(config).await?;
        Ok(())
    }
}

impl<S: crate::Services> DefaultConfigurationResolver<S> {
    async fn check_agent_model_compatibility(
        &self,
        agent: &AgentId,
        _model: &ModelId,
    ) -> Result<Option<ConfigError>, ConfigError> {
        // Get the agent to check its capabilities
        let agents = self.services.get_agents().await?;
        let _agent_info = agents.iter().find(|a| a.id == *agent);

        // TODO: Implement agent-model compatibility checking
        // For now, we'll assume all agents are compatible with all models
        // In the future, we might check agent.allowed_models if it exists

        Ok(None)
    }
}

fn parse_env(env: Vec<String>) -> std::collections::BTreeMap<String, String> {
    env.into_iter()
        .filter_map(|line| {
            line.split_once('=')
                .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use forge_domain::{AgentId, ModelId};

    use super::*;
    use crate::dto::AppConfig;

    #[test]
    fn test_parse_env() {
        let env = vec![
            "FORGE_AGENT=test-agent".to_string(),
            "FORGE_MODEL=test-model".to_string(),
            "OTHER_VAR=value".to_string(),
        ];

        let result = parse_env(env);
        assert_eq!(result.get("FORGE_AGENT"), Some(&"test-agent".to_string()));
        assert_eq!(result.get("FORGE_MODEL"), Some(&"test-model".to_string()));
        assert_eq!(result.get("OTHER_VAR"), Some(&"value".to_string()));
    }

    #[test]
    fn test_config_source_precedence() {
        assert!(ConfigSource::Environment > ConfigSource::AppConfig);
        assert!(ConfigSource::AppConfig > ConfigSource::Default);
    }

    #[test]
    fn test_resolved_config_creation() {
        let config = ResolvedConfig {
            agent: Some((AgentId::new("test-agent"), ConfigSource::Environment)),
            model: Some((ModelId::new("test-model"), ConfigSource::AppConfig)),
        };

        assert_eq!(config.agent.unwrap().1, ConfigSource::Environment);
        assert_eq!(config.model.unwrap().1, ConfigSource::AppConfig);
    }
}
