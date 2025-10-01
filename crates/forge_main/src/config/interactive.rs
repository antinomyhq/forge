use std::fmt::Display;

use anyhow::Result;
use forge_api::{API, Agent, AgentId, Model, ModelId, Provider, ProviderId};

use crate::select::ForgeSelect;

/// Wrapper for displaying agents in selection menu
struct CliAgent(Agent);

impl Display for CliAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.id)?;
        if let Some(title) = &self.0.title {
            write!(f, " - {}", title)?;
        }
        Ok(())
    }
}

/// Wrapper for displaying models in selection menu
struct CliModel(Model);

impl Display for CliModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.id)?;

        let mut info_parts = Vec::new();

        // Add context length if available
        if let Some(limit) = self.0.context_length {
            if limit >= 1_000_000 {
                info_parts.push(format!("{}M", limit / 1_000_000));
            } else if limit >= 1000 {
                info_parts.push(format!("{}k", limit / 1000));
            } else {
                info_parts.push(format!("{limit}"));
            }
        }

        // Add tools support indicator if explicitly supported
        if self.0.tools_supported == Some(true) {
            info_parts.push("🛠️".to_string());
        }

        // Only show brackets if we have info to display
        if !info_parts.is_empty() {
            use colored::Colorize;
            let info = format!("[ {} ]", info_parts.join(" "));
            write!(f, " {}", info.dimmed())?;
        }

        Ok(())
    }
}

/// Wrapper for displaying providers in selection menu
struct CliProvider(Provider);

impl Display for CliProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = self.0.id.to_string();
        write!(f, "{}", name)?;
        if let Some(domain) = self.0.url.domain() {
            write!(f, " [{}]", domain)?;
        }
        Ok(())
    }
}

/// Interactive agent selection
pub async fn select_agent<A: API>(api: &A) -> Result<Option<AgentId>> {
    // Fetch available agents
    let mut agents = api
        .get_agents()
        .await?
        .into_iter()
        .map(CliAgent)
        .collect::<Vec<_>>();

    // Sort by agent ID
    agents.sort_by(|a, b| a.0.id.as_str().cmp(b.0.id.as_str()));

    // Get current agent to set starting cursor
    let current_agent = api.get_operating_agent().await;
    let starting_cursor = current_agent
        .as_ref()
        .and_then(|current| agents.iter().position(|a| &a.0.id == current))
        .unwrap_or(0);

    // Show selection
    match ForgeSelect::select("Select an agent:", agents)
        .with_starting_cursor(starting_cursor)
        .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
        .prompt()?
    {
        Some(agent) => Ok(Some(agent.0.id)),
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
        .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
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
        .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
        .prompt()?
    {
        Some(provider) => Ok(Some(provider.0.id)),
        None => Ok(None),
    }
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

    ForgeSelect::select("What would you like to configure?", options)
        .with_help_message("Use arrow keys to navigate and Enter to select")
        .prompt()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_config_option_display() {
        let fixture = ConfigOption::Agent;
        let actual = format!("{}", fixture);
        let expected = "Agent";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_agent_display_with_title() {
        let fixture = Agent::new(AgentId::FORGE).title("Test Agent");
        let actual = format!("{}", CliAgent(fixture));
        assert_eq!(actual.contains("forge"), true);
        assert_eq!(actual.contains("Test Agent"), true);
    }

    #[test]
    fn test_cli_agent_display_without_title() {
        let fixture = Agent::new(AgentId::FORGE);
        let actual = format!("{}", CliAgent(fixture));
        assert_eq!(actual, "forge");
    }
}
