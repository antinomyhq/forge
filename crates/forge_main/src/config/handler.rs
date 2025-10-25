use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_api::{API, AgentId, ModelId, ProviderId};

use super::display::{display_single_field, display_success};
use super::error::{ConfigError, Result as ConfigResult};
use crate::cli::{ConfigCommand, ConfigGetArgs, ConfigSetArgs};

/// Configuration manager that handles all config operations
pub struct ConfigManager<A> {
    api: Arc<A>,
}

impl<A: API> ConfigManager<A> {
    /// Create a new ConfigManager with the given API reference
    pub fn new(api: Arc<A>) -> Self {
        Self { api }
    }

    /// Handle config command
    pub async fn handle_command(&self, command: ConfigCommand, porcelain: bool) -> Result<()> {
        match command {
            ConfigCommand::Set(args) => self.handle_set(args).await?,
            ConfigCommand::Get(args) => self.handle_get(args).await?,
            ConfigCommand::List => self.handle_list(porcelain).await?,
        }
        Ok(())
    }

    /// Handle config set command
    async fn handle_set(&self, args: ConfigSetArgs) -> ConfigResult<()> {
        if args.has_any_field() {
            // Non-interactive mode: set specified values
            self.handle_non_interactive_set(args).await
        } else {
            Ok(())
        }
    }

    /// Handle non-interactive config set
    async fn handle_non_interactive_set(&self, args: ConfigSetArgs) -> ConfigResult<()> {
        // Set provider if specified
        if let Some(provider_str) = args.provider {
            let provider_id = self.validate_provider(&provider_str).await?;
            self.api.set_provider(provider_id).await?;
            display_success("Provider set", &provider_str);
        }

        // Set agent if specified
        if let Some(agent_str) = args.agent {
            let agent_id = self.validate_agent(&agent_str).await?;
            self.api.set_operating_agent(agent_id.clone()).await?;
            display_success("Agent set", agent_id.as_str());
        }

        // Set model if specified
        if let Some(model_str) = args.model {
            let model_id = self.validate_model(&model_str).await?;
            self.api.set_operating_model(model_id.clone()).await?;
            display_success("Model set", model_id.as_str());
        }

        Ok(())
    }

    /// Handle config get command
    async fn handle_get(&self, args: ConfigGetArgs) -> ConfigResult<()> {
        use crate::cli::ConfigField;

        // Get specific field
        match args.field {
            ConfigField::Agent => {
                let agent = self
                    .api
                    .get_operating_agent()
                    .await
                    .map(|a| a.as_str().to_string());
                display_single_field("agent", agent);
            }
            ConfigField::Model => {
                let model = self
                    .api
                    .get_operating_model()
                    .await
                    .map(|m| m.as_str().to_string());
                display_single_field("model", model);
            }
            ConfigField::Provider => {
                let provider = self.api.get_provider().await.ok().map(|p| p.id.to_string());
                display_single_field("provider", provider);
            }
        }

        Ok(())
    }

    /// Handle config list command
    async fn handle_list(&self, porcelain: bool) -> ConfigResult<()> {
        let agent = self
            .api
            .get_operating_agent()
            .await
            .map(|a| a.as_str().to_string());
        let model = self
            .api
            .get_operating_model()
            .await
            .map(|m| m.as_str().to_string());
        let provider = self.api.get_provider().await.ok().map(|p| p.id.to_string());
        super::helpers::build_config_info(agent, model, provider, porcelain);
        Ok(())
    }

    /// Validate agent exists
    async fn validate_agent(&self, agent_str: &str) -> ConfigResult<AgentId> {
        let agents = self.api.get_agents().await?;
        let agent_id = AgentId::new(agent_str);

        if agents.iter().any(|a| a.id == agent_id) {
            Ok(agent_id)
        } else {
            let available: Vec<_> = agents.iter().map(|a| a.id.as_str()).collect();
            Err(ConfigError::AgentNotFound {
                agent: agent_str.to_string(),
                available: available.join(", "),
            })
        }
    }

    /// Validate model exists
    async fn validate_model(&self, model_str: &str) -> ConfigResult<ModelId> {
        let models = self.api.models().await?;
        let model_id = ModelId::new(model_str);

        if models.iter().any(|m| m.id == model_id) {
            Ok(model_id)
        } else {
            // Show first 10 models as suggestions
            let available: Vec<_> = models.iter().take(10).map(|m| m.id.as_str()).collect();
            let suggestion = if models.len() > 10 {
                format!("{} (and {} more)", available.join(", "), models.len() - 10)
            } else {
                available.join(", ")
            };

            Err(ConfigError::ModelNotFound { model: model_str.to_string(), available: suggestion })
        }
    }

    /// Validate provider exists and has API key
    async fn validate_provider(&self, provider_str: &str) -> ConfigResult<ProviderId> {
        // Parse provider ID from string
        let provider_id = ProviderId::from_str(provider_str).with_context(|| {
            format!(
                "Invalid provider: '{}'. Valid providers are: {}",
                provider_str,
                get_valid_provider_names().join(", ")
            )
        })?;

        // Check if provider has valid API key
        let providers = self.api.providers().await?;
        if providers.iter().any(|p| p.id == provider_id) {
            Ok(provider_id)
        } else {
            Err(ConfigError::ProviderNotAvailable {
                provider: provider_str.to_string(),
                available: providers
                    .iter()
                    .map(|p| p.id.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
        }
    }
}

/// Get list of valid provider names
fn get_valid_provider_names() -> Vec<String> {
    use strum::IntoEnumIterator;
    ProviderId::iter().map(|p| p.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_get_valid_provider_names() {
        let fixture = get_valid_provider_names();
        let actual = !fixture.is_empty();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_args_has_any_field() {
        let fixture = ConfigSetArgs {
            agent: Some("forge".to_string()),
            model: None,
            provider: None,
        };
        let actual = fixture.has_any_field();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_args_has_no_field() {
        let fixture = ConfigSetArgs { agent: None, model: None, provider: None };
        let actual = fixture.has_any_field();
        let expected = false;
        assert_eq!(actual, expected);
    }
}
