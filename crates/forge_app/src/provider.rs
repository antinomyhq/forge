use std::sync::Arc;

use forge_domain::{ModelId, Provider};

use crate::{AgentLoaderService, ProviderRegistry};

pub struct ProviderManager<S> {
    services: Arc<S>,
}

impl<S: ProviderRegistry + AgentLoaderService> ProviderManager<S> {
    pub fn new(services: Arc<S>) -> Self {
        ProviderManager { services }
    }
    pub async fn get_active_provider(&self) -> anyhow::Result<Provider> {
        let agents = self.services.get_agents().await?;
        if let Some(provider_id) = self
            .services
            .get_active_agent()
            .await?
            .and_then(|agent_id| agents.into_iter().find(|v| v.id == agent_id))
            .and_then(|agent| agent.provider)
        {
            return self.services.provider_from_id(provider_id).await;
        }

        // fall back to original logic if there is no agent
        // set yet.
        self.services.get_active_provider().await
    }
    pub async fn get_active_model(&self) -> anyhow::Result<ModelId> {
        let provider_id = self.get_active_provider().await?.id;
        self.services.get_active_model(&provider_id).await
    }
    pub async fn set_active_model(&self, model: ModelId) -> anyhow::Result<()> {
        let provider_id = self.get_active_provider().await?.id;
        self.services.set_active_model(model, provider_id).await
    }
}
