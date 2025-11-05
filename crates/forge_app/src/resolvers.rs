use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{AgentId, ConfigScope, ModelId, Provider, ProviderId, ScopeResolution, Trace};
use url::Url;

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
impl<S> ScopeResolution for ProviderResolver<S>
where
    S: Services + AgentRegistry + AppConfigService + ProviderService,
{
    type Config = Provider<Url>;

    async fn get_global_level(&self) -> Result<Option<Trace<Self::Config>>> {
        let provider = self.services.get_default_provider().await?;
        Ok(Some(Trace::new(provider).add_trace("global")))
    }

    async fn get_project_level(&self) -> Result<Option<Trace<Self::Config>>> {
        Ok(None)
    }

    async fn get_provider_level(&self, id: &ProviderId) -> Result<Option<Trace<Self::Config>>> {
        let provider = self.services.get_provider(*id).await?;
        Ok(Some(Trace::new(provider).add_trace("provider")))
    }

    async fn get_agent_level(&self, id: &AgentId) -> Result<Option<Trace<Self::Config>>> {
        let agent = self
            .services
            .get_agent(id)
            .await?
            .context("Agent not found")?;

        if let Some(provider_id) = agent.provider {
            let provider = self.services.get_provider(provider_id).await?;
            Ok(Some(Trace::new(provider).add_trace("agent")))
        } else {
            Ok(None)
        }
    }

    async fn set_global_level(&self, config: Self::Config) -> Result<Option<()>> {
        Ok(Some(self.services.set_default_provider(config.id).await?))
    }

    async fn set_project_level(&self, _config: Self::Config) -> Result<Option<()>> {
        Ok(None)
    }

    async fn set_provider_level(
        &self,
        _id: &ProviderId,
        _config: Self::Config,
    ) -> Result<Option<()>> {
        Ok(None)
    }

    async fn set_agent_level(&self, _id: &AgentId, _config: Self::Config) -> Result<Option<()>> {
        Ok(None)
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
impl<S> ScopeResolution for ModelResolver<S>
where
    S: Services + AgentRegistry + AppConfigService,
{
    type Config = ModelId;
    async fn get_global_level(&self) -> Result<Option<Trace<Self::Config>>> {
        let provider = self.services.get_default_provider().await?;
        let model = self.services.get_default_model(&provider.id).await?;
        Ok(Some(Trace::new(model).add_trace("global")))
    }

    async fn get_project_level(&self) -> Result<Option<Trace<Self::Config>>> {
        Ok(None)
    }

    async fn get_provider_level(&self, _id: &ProviderId) -> Result<Option<Trace<Self::Config>>> {
        Ok(None)
    }

    async fn get_agent_level(&self, id: &AgentId) -> Result<Option<Trace<Self::Config>>> {
        let agent = self
            .services
            .get_agent(id)
            .await?
            .context("Agent not found")?;

        if let Some(model) = agent.model {
            Ok(Some(Trace::new(model).add_trace("agent")))
        } else {
            Ok(None)
        }
    }

    async fn set_global_level(&self, config: Self::Config) -> Result<Option<()>> {
        let provider = self.services.get_default_provider().await?;
        Ok(Some(
            self.services.set_default_model(config, provider.id).await?,
        ))
    }

    async fn set_project_level(&self, _config: Self::Config) -> Result<Option<()>> {
        Ok(None)
    }

    async fn set_provider_level(
        &self,
        _id: &ProviderId,
        _config: Self::Config,
    ) -> Result<Option<()>> {
        Ok(None)
    }

    async fn set_agent_level(&self, _id: &AgentId, _config: Self::Config) -> Result<Option<()>> {
        Ok(None)
    }
}
