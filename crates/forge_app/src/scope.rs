use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{AgentId, ModelId, Provider, ProviderId, ScopeGetter, ScopeSetter};
use url::Url;

use crate::{AgentRegistry, AppConfigService, ProviderService, Services};

/// Resolves provider configuration based on scope
pub struct ProviderScope<S> {
    services: Arc<S>,
}

impl<S> ProviderScope<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
}

#[async_trait::async_trait]
impl<S> ScopeGetter for ProviderScope<S>
where
    S: Services + AgentRegistry + AppConfigService + ProviderService,
{
    type Config = Provider<Url>;

    async fn get_global_level(&self) -> Result<Option<Self::Config>> {
        let provider = self.services.get_default_provider().await?;
        Ok(Some(provider))
    }

    async fn get_project_level(&self) -> Result<Option<Self::Config>> {
        Ok(None)
    }

    async fn get_provider_level(&self, id: &ProviderId) -> Result<Option<Self::Config>> {
        let provider = self.services.get_provider(*id).await?;
        Ok(Some(provider))
    }

    async fn get_agent_level(&self, id: &AgentId) -> Result<Option<Self::Config>> {
        let agent = self
            .services
            .get_agent(id)
            .await?
            .context("Agent not found")?;

        if let Some(provider_id) = agent.provider {
            let provider = self.services.get_provider(provider_id).await?;
            Ok(Some(provider))
        } else {
            Ok(None)
        }
    }
}

#[async_trait::async_trait]
impl<S> ScopeSetter for ProviderScope<S>
where
    S: Services + AgentRegistry + AppConfigService + ProviderService,
{
    type Config = Provider<Url>;

    async fn set_global_level(&self, config: Self::Config) -> Result<bool> {
        self.services.set_default_provider(config.id).await?;
        Ok(true)
    }

    async fn set_project_level(&self, _config: Self::Config) -> Result<bool> {
        Ok(false)
    }

    async fn set_provider_level(&self, _id: &ProviderId, _config: Self::Config) -> Result<bool> {
        Ok(false)
    }

    async fn set_agent_level(&self, _id: &AgentId, _config: Self::Config) -> Result<bool> {
        Ok(false)
    }
}

/// Resolves model configuration based on scope
pub struct ModelScope<S> {
    services: Arc<S>,
}

impl<S> ModelScope<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
}

#[async_trait::async_trait]
impl<S> ScopeGetter for ModelScope<S>
where
    S: Services + AgentRegistry + AppConfigService,
{
    type Config = ModelId;
    async fn get_global_level(&self) -> Result<Option<Self::Config>> {
        let provider = self.services.get_default_provider().await?;
        let model = self.services.get_default_model(&provider.id).await?;
        Ok(Some(model))
    }

    async fn get_project_level(&self) -> Result<Option<Self::Config>> {
        Ok(None)
    }

    async fn get_provider_level(&self, id: &ProviderId) -> Result<Option<Self::Config>> {
        let model = self.services.get_default_model(&id).await?;
        Ok(Some(model))
    }

    async fn get_agent_level(&self, id: &AgentId) -> Result<Option<Self::Config>> {
        let agent = self
            .services
            .get_agent(id)
            .await?
            .context("Agent not found")?;

        if let Some(model) = agent.model {
            Ok(Some(model))
        } else {
            Ok(None)
        }
    }
}

#[async_trait::async_trait]
impl<S> ScopeSetter for ModelScope<S>
where
    S: Services + AgentRegistry + AppConfigService,
{
    type Config = ModelId;

    async fn set_global_level(&self, config: Self::Config) -> Result<bool> {
        let provider = self.services.get_default_provider().await?;
        self.services.set_default_model(config, provider.id).await?;
        Ok(true)
    }

    async fn set_project_level(&self, _config: Self::Config) -> Result<bool> {
        Ok(false)
    }

    async fn set_provider_level(&self, _id: &ProviderId, _config: Self::Config) -> Result<bool> {
        Ok(false)
    }

    async fn set_agent_level(&self, _id: &AgentId, _config: Self::Config) -> Result<bool> {
        Ok(false)
    }
}
