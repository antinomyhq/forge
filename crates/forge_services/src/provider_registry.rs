use std::sync::Arc;

use anyhow::Context;
use forge_app::{AppConfig, ProviderRegistry};
use forge_domain::{Provider, ProviderConfig, ProviderDetails};
use tokio::sync::RwLock;

use crate::{provider, EnvironmentInfra};

pub struct ForgeProviderRegistry<F> {
    infra: Arc<F>,
    // IMPORTANT: This cache is used to avoid logging out if the user has logged out from other
    // session. This helps to keep the user logged in for current session.
    cache: Arc<RwLock<Option<Provider>>>,
}

impl<F: EnvironmentInfra> ForgeProviderRegistry<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Arc::new(Default::default()) }
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra> ProviderRegistry for ForgeProviderRegistry<F> {
    async fn get_provider(&self, app_config: AppConfig) -> anyhow::Result<Provider> {
        let provider = self.cache.read().await;
        if let Some(cached_provider) = provider.as_ref() {
            return Ok(cached_provider.clone());
        } else {
            drop(provider);
            let provider_config = ProviderConfig::default();
            
            let resolved_config = provider::resolve_env_provider(&provider_config, self.infra.as_ref());
            let mut new_provider_config = ProviderConfig {
                provider_id: None,
                providers: resolved_config,
            };
            // Check if a provider with forge is already there
            let has_forge_provider = new_provider_config.providers.iter()
                .any(|p| p.id.to_lowercase().contains("forge"));

            if !has_forge_provider {
                if let Some(login) = app_config.key_info {
                    let forge_provider = ProviderDetails::new(
                        "forge".to_string(),
                        "Forge".to_string(),
                        "Forge AI Provider".to_string(),
                        login.api_key,
                        "openai".to_string(),
                        " https://antinomy.ai/api/v1/".to_string(),
                    );
                    new_provider_config.providers.push(forge_provider);
                }
            }
            if !new_provider_config.providers.is_empty() {
                new_provider_config.provider_id = Some(new_provider_config.providers[0].id.clone());
            }
            let provider = new_provider_config.get_provider().context("Failed to detect provider")?;
            self.cache.write().await.replace(provider.clone());
            return Ok(provider);
        }
    }

    async fn update_provider(&self, provider: Provider) -> anyhow::Result<()> {
        // Update the cache with the new provider
        *self.cache.write().await = Some(provider);
        Ok(())
    }
}
