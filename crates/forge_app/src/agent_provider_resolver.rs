use std::sync::Arc;

use anyhow::Result;
use forge_domain::{AgentId, ModelId, Provider};

use crate::{AgentRegistry, AppConfigService, ProviderAuthService, ProviderService};

/// Resolver for agent providers and models.
/// Handles provider resolution, credential refresh, and model lookup.
pub struct AgentProviderResolver<S>(Arc<S>);

impl<S> AgentProviderResolver<S> {
    /// Creates a new AgentProviderResolver instance
    pub fn new(services: Arc<S>) -> Self {
        Self(services)
    }
}

impl<S> AgentProviderResolver<S>
where
    S: AgentRegistry + ProviderService + AppConfigService + ProviderAuthService,
{
    /// Gets the provider for the specified agent, or the default provider if no
    /// agent is provided. Automatically refreshes OAuth credentials if they're
    /// about to expire.
    pub async fn get_provider(&self, agent_id: Option<AgentId>) -> Result<Provider<url::Url>> {
        let provider_id = if let Some(agent_id) = agent_id {
            // Load all agent definitions and find the one we need

            if let Some(agent) = self.0.get_agent(&agent_id).await? {
                // If the agent definition has a provider, use it; otherwise use default
                agent.provider
            } else {
                // TODO: Needs review, should we throw an err here?
                // we can throw crate::Error::AgentNotFound
                self.0.get_default_provider().await?
            }
        } else {
            self.0.get_default_provider().await?
        };

        let provider = self.0.get_provider(provider_id).await?;
        Ok(provider)
    }

    /// Gets the model for the specified agent, or the default model if no agent
    /// is provided.
    ///
    /// Priority: agent's configured model > active agent's model > global
    /// provider model.
    pub async fn get_model(&self, agent_id: Option<AgentId>) -> Result<ModelId> {
        // Resolve the effective agent: explicit arg > active agent
        let resolved = match agent_id {
            Some(id) => Some(id),
            None => self.0.get_active_agent_id().await.ok().flatten(),
        };

        if let Some(agent_id) = resolved {
            if let Some(agent) = self.0.get_agent(&agent_id).await? {
                return Ok(agent.model);
            }
        }

        // Fall back to the global model set for the active provider
        let provider_id = self.get_provider(None).await?.id;
        Ok(self.0.get_provider_model(Some(&provider_id)).await?)
    }
}
