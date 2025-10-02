use std::fmt::Display;
use std::str::FromStr;

use anyhow::{Context, Result};
use forge_api::{API, AgentId, ModelId, ProviderId};

use super::display::{display_all_config, display_single_field, display_success};
use super::error::{ConfigError, Result as ConfigResult};
use crate::cli::{ConfigCommand, ConfigGetArgs, ConfigSetArgs};
use crate::select::ForgeSelect;
use crate::ui_components::{CLIAgent, CliModel, CliProvider};

/// Handle config command
pub async fn handle_config_command<A: API>(api: &A, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Set(args) => handle_config_set(api, args).await?,
        ConfigCommand::Get(args) => handle_config_get(api, args).await?,
    }
    Ok(())
}

/// Handle config set command
async fn handle_config_set<A: API>(api: &A, args: ConfigSetArgs) -> ConfigResult<()> {
    if args.has_any_field() {
        // Non-interactive mode: set specified values
        handle_non_interactive_set(api, args).await
    } else {
        // Interactive mode: show menu
        handle_interactive_set(api).await
    }
}

/// Handle non-interactive config set
async fn handle_non_interactive_set<A: API>(api: &A, args: ConfigSetArgs) -> ConfigResult<()> {
    // Set provider if specified
    if let Some(provider_str) = args.provider {
        let provider_id = validate_provider(api, &provider_str).await?;
        api.set_provider(provider_id).await?;
        display_success("Provider set", &provider_str);
    }

    // Set agent if specified
    if let Some(agent_str) = args.agent {
        let agent_id = validate_agent(api, &agent_str).await?;
        api.set_operating_agent(agent_id.clone()).await?;
        display_success("Agent set", agent_id.as_str());
    }

    // Set model if specified
    if let Some(model_str) = args.model {
        let model_id = validate_model(api, &model_str).await?;
        api.set_operating_model(model_id.clone()).await?;
        display_success("Model set", model_id.as_str());
    }

    Ok(())
}

/// Handle interactive config set
async fn handle_interactive_set<A: API>(api: &A) -> ConfigResult<()> {
    // Show menu to select what to configure
    let option = match show_config_menu()? {
        Some(opt) => opt,
        None => return Ok(()), // User canceled
    };

    match option {
        ConfigOption::Agent => {
            if let Some(agent_id) = select_agent(api).await? {
                api.set_operating_agent(agent_id.clone()).await?;
                display_success("Agent set", agent_id.as_str());
            }
        }
        ConfigOption::Model => {
            if let Some(model_id) = select_model(api).await? {
                api.set_operating_model(model_id.clone()).await?;
                display_success("Model set", model_id.as_str());
            }
        }
        ConfigOption::Provider => {
            if let Some(provider_id) = select_provider(api).await? {
                api.set_provider(provider_id).await?;
                display_success("Provider set", &provider_id.to_string());
            }
        }
    }

    Ok(())
}

/// Handle config get command
async fn handle_config_get<A: API>(api: &A, args: ConfigGetArgs) -> ConfigResult<()> {
    if let Some(field) = args.field {
        // Get specific field
        match field.to_lowercase().as_str() {
            "agent" => {
                let agent = api
                    .get_operating_agent()
                    .await
                    .map(|a| a.as_str().to_string());
                display_single_field("agent", agent);
            }
            "model" => {
                let model = api
                    .get_operating_model()
                    .await
                    .map(|m| m.as_str().to_string());
                display_single_field("model", model);
            }
            "provider" => {
                let provider = api.get_provider().await.ok().map(|p| p.id.to_string());
                display_single_field("provider", provider);
            }
            _ => {
                return Err(ConfigError::InvalidField { field: field.clone() });
            }
        }
    } else {
        // Get all configuration
        let agent = api
            .get_operating_agent()
            .await
            .map(|a| a.as_str().to_string());
        let model = api
            .get_operating_model()
            .await
            .map(|m| m.as_str().to_string());
        let provider = api.get_provider().await.ok().map(|p| p.id.to_string());

        display_all_config(agent, model, provider);
    }

    Ok(())
}

/// Validate agent exists
async fn validate_agent<A: API>(api: &A, agent_str: &str) -> ConfigResult<AgentId> {
    let agents = api.get_agents().await?;
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
async fn validate_model<A: API>(api: &A, model_str: &str) -> ConfigResult<ModelId> {
    let models = api.models().await?;
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
async fn validate_provider<A: API>(api: &A, provider_str: &str) -> ConfigResult<ProviderId> {
    // Parse provider ID from string
    let provider_id = ProviderId::from_str(provider_str).with_context(|| {
        format!(
            "Invalid provider: '{}'. Valid providers are: {}",
            provider_str,
            get_valid_provider_names().join(", ")
        )
    })?;

    // Check if provider has valid API key
    let providers = api.providers().await?;
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

/// Get list of valid provider names
fn get_valid_provider_names() -> Vec<String> {
    use strum::IntoEnumIterator;
    ProviderId::iter().map(|p| p.to_string()).collect()
}

/// Configuration option for interactive menu
#[derive(Debug, Clone)]
pub enum ConfigOption {
    Agent,
    Model,
    Provider,
}

impl Display for ConfigOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigOption::Agent => write!(f, "Agent"),
            ConfigOption::Model => write!(f, "Model"),
            ConfigOption::Provider => write!(f, "Provider"),
        }
    }
}

/// Show interactive menu to select what to configure
pub fn show_config_menu() -> Result<Option<ConfigOption>> {
    let options = vec![
        ConfigOption::Agent,
        ConfigOption::Model,
        ConfigOption::Provider,
    ];

    ForgeSelect::select("What would you like to configure?", options).prompt()
}

/// Interactive agent selection
pub async fn select_agent<A: API>(api: &A) -> Result<Option<AgentId>> {
    // Fetch available agents
    let agents = api.get_agents().await?;

    // Find the maximum agent ID length for consistent padding
    let max_id_length = agents
        .iter()
        .map(|agent| agent.id.as_str().len())
        .max()
        .unwrap_or(0);

    // Convert to SelectionAgent with formatted labels
    let mut display_agents = agents
        .into_iter()
        .map(|agent| CLIAgent::from_agent_with_label(&agent, max_id_length))
        .collect::<Vec<_>>();

    // Sort by agent ID
    display_agents.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

    // Get current agent to set starting cursor
    let current_agent = api.get_operating_agent().await;
    let starting_cursor = current_agent
        .as_ref()
        .and_then(|current| display_agents.iter().position(|a| a.id == *current))
        .unwrap_or(0);

    // Show selection
    match ForgeSelect::select("Select an agent:", display_agents)
        .with_starting_cursor(starting_cursor)
        .prompt()?
    {
        Some(agent) => Ok(Some(agent.id)),
        None => Ok(None),
    }
}

/// Interactive model selection
pub async fn select_model<A: API>(api: &A) -> Result<Option<ModelId>> {
    // Fetch available models
    let mut models = api
        .models()
        .await?
        .into_iter()
        .map(CliModel)
        .collect::<Vec<_>>();

    // Sort alphabetically
    models.sort_by(|a, b| a.0.id.as_str().cmp(b.0.id.as_str()));

    // Get current model to set starting cursor
    let current_model = api.get_operating_model().await;
    let starting_cursor = current_model
        .as_ref()
        .and_then(|current| models.iter().position(|m| &m.0.id == current))
        .unwrap_or(0);

    // Show selection
    match ForgeSelect::select("Select a model:", models)
        .with_starting_cursor(starting_cursor)
        .prompt()?
    {
        Some(model) => Ok(Some(model.0.id)),
        None => Ok(None),
    }
}

/// Interactive provider selection
pub async fn select_provider<A: API>(api: &A) -> Result<Option<ProviderId>> {
    // Fetch available providers
    let mut providers = api
        .providers()
        .await?
        .into_iter()
        .map(CliProvider)
        .collect::<Vec<_>>();

    // Sort by display name
    providers.sort_by_key(|p| p.0.id.to_string());

    // Get current provider to set starting cursor
    let current_provider = api.get_provider().await.ok();
    let starting_cursor = current_provider
        .as_ref()
        .and_then(|current| providers.iter().position(|p| p.0.id == current.id))
        .unwrap_or(0);

    // Show selection
    match ForgeSelect::select("Select a provider:", providers)
        .with_starting_cursor(starting_cursor)
        .prompt()?
    {
        Some(provider) => Ok(Some(provider.0.id)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_get_valid_provider_names() {
        let fixture = get_valid_provider_names();
        let actual = fixture.len() > 0;
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

    #[test]
    fn test_config_option_display() {
        let fixture = ConfigOption::Agent;
        let actual = format!("{}", fixture);
        let expected = "Agent";
        assert_eq!(actual, expected);
    }
}
