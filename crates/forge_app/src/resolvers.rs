use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{ConfigScope, ModelId, Provider, ResolveScopeConfig};

use crate::{AgentRegistry, AppConfigService, ProviderService, Services};

/// Resolves provider configuration based on scope
pub struct ProviderResolver<S> {
    services: Arc<S>,
}

impl<S> ProviderResolver<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
}

#[async_trait::async_trait]
impl<S> ResolveScopeConfig for ProviderResolver<S>
where
    S: Services + AgentRegistry + AppConfigService + ProviderService,
{
    type Config = Provider;

    async fn get(&self, scope: &ConfigScope) -> Result<Option<Self::Config>> {
        match scope {
            ConfigScope::Global => {
                let provider = self.services.get_default_provider().await?;
                Ok(Some(provider))
            }
            ConfigScope::Agent(agent_id) => {
                let agent = self
                    .services
                    .get_agent(agent_id)
                    .await?
                    .context("Agent not found")?;

                if let Some(provider_id) = agent.provider {
                    let provider = self.services.get_provider(provider_id).await?;
                    Ok(Some(provider))
                } else {
                    Ok(None)
                }
            }
            ConfigScope::Or(first, second) => {
                if let Some(provider) = self.get(first).await? {
                    Ok(Some(provider))
                } else {
                    self.get(second).await
                }
            }
            _ => Ok(None),
        }
    }

    async fn set(&self, scope: &ConfigScope, config: Self::Config) -> Result<()> {
        match scope {
            ConfigScope::Global => {
                self.services.set_default_provider(config.id).await?;
                Ok(())
            }
            _ => anyhow::bail!(
                "Setting provider only supported for Global scope (agents are read-only)"
            ),
        }
    }
}

/// Resolves model configuration based on scope
pub struct ModelResolver<S> {
    services: Arc<S>,
}

impl<S> ModelResolver<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
}

#[async_trait::async_trait]
impl<S> ResolveScopeConfig for ModelResolver<S>
where
    S: Services + AgentRegistry + AppConfigService,
{
    type Config = ModelId;

    async fn get(&self, scope: &ConfigScope) -> Result<Option<Self::Config>> {
        match scope {
            ConfigScope::Global => {
                // Get the default provider first
                let provider = self.services.get_default_provider().await?;
                self.services
                    .get_default_model(&provider.id)
                    .await
                    .map(Some)
            }
            ConfigScope::Agent(agent_id) => {
                let agent = self
                    .services
                    .get_agent(agent_id)
                    .await?
                    .context("Agent not found")?;
                Ok(agent.model)
            }
            ConfigScope::Or(first, second) => {
                if let Some(model) = self.get(first).await? {
                    Ok(Some(model))
                } else {
                    self.get(second).await
                }
            }
            _ => Ok(None),
        }
    }

    async fn set(&self, scope: &ConfigScope, config: Self::Config) -> Result<()> {
        match scope {
            ConfigScope::Global => {
                // Get the default provider to associate the model with
                let provider = self.services.get_default_provider().await?;
                self.services.set_default_model(config, provider.id).await?;
                Ok(())
            }
            _ => anyhow::bail!(
                "Setting model only supported for Global scope (agents are read-only)"
            ),
        }
    }
}
