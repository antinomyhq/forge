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
        let provider = if let Some(agent_id) = agent_id {
            // Load all agent definitions and find the one we need

            if let Some(agent) = self.0.get_agent(&agent_id).await? {
                // If the agent definition has a provider, use it; otherwise use default
                self.0.get_provider(agent.provider).await?
            } else {
                // TODO: Needs review, should we throw an err here?
                // we can throw crate::Error::AgentNotFound
                self.0.get_default_provider().await?
            }
        } else {
            self.0.get_default_provider().await?
        };

        // Check if credential needs refresh (5 minute buffer before expiry)
        if let Some(credential) = &provider.credential {
            let buffer = chrono::Duration::minutes(5);

            if credential.needs_refresh(buffer) {
                for auth_method in &provider.auth_methods {
                    match auth_method {
                        forge_domain::AuthMethod::OAuthDevice(_)
                        | forge_domain::AuthMethod::OAuthCode(_) => {
                            match self
                                .0
                                .refresh_provider_credential(&provider, auth_method.clone())
                                .await
                            {
                                Ok(refreshed_credential) => {
                                    let mut updated_provider = provider.clone();
                                    updated_provider.credential = Some(refreshed_credential);
                                    return Ok(updated_provider);
                                }
                                Err(_) => {
                                    return Ok(provider);
                                }
                            }
                        }
                        forge_domain::AuthMethod::ApiKey => {}
                    }
                }
            }
        }

        Ok(provider)
    }

    /// Gets the model for the specified agent, or the default model if no agent
    /// is provided
    pub async fn get_model(&self, agent_id: Option<AgentId>) -> Result<ModelId> {
        if let Some(agent_id) = agent_id {
            if let Some(agent) = self.0.get_agent(&agent_id).await? {
                Ok(agent.model)
            } else {
                // TODO: Needs review, should we throw an err here?
                // we can throw crate::Error::AgentNotFound
                let provider_id = self.get_provider(Some(agent_id)).await?.id;
                self.0.get_default_model(&provider_id).await
            }
        } else {
            let provider_id = self.get_provider(None).await?.id;
            self.0.get_default_model(&provider_id).await
        }
    }

    /// Sets the model for the agent's provider
    pub async fn set_default_model(&self, agent_id: Option<AgentId>, model: ModelId) -> Result<()> {
        // Invalidate cache for agents
        let result = if let Some(agent_id) = agent_id {
            if let Some(agent) = self.0.get_agent(&agent_id).await? {
                let provider_id = agent.provider;
                self.0.set_default_model(model, provider_id).await
            } else {
                // TODO: Needs review, should we throw an err here?
                // we can throw crate::Error::AgentNotFound
                let provider_id = self.get_provider(None).await?.id;
                self.0.set_default_model(model, provider_id).await
            }
        } else {
            let provider_id = self.get_provider(None).await?.id;
            self.0.set_default_model(model, provider_id).await
        };
        self.0.reload_agents().await?;

        result
    }
}
