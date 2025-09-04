use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::{Provider, ProviderUrl};
use forge_app::{ProfileService, ProviderRegistry};
use tokio::sync::RwLock;

use crate::EnvironmentInfra;

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

pub struct ForgeProviderRegistry<F, P> {
    infra: Arc<F>,
    profile_service: Arc<P>,
    // IMPORTANT: This cache is used to avoid logging out if the user has logged out from other
    // session. This helps to keep the user logged in for current session.
    cache: Arc<RwLock<Option<Provider>>>,
}

impl<F: EnvironmentInfra, P: ProfileService> ForgeProviderRegistry<F, P> {
    pub fn new(infra: Arc<F>, profile_service: Arc<P>) -> Self {
        Self { infra, profile_service, cache: Arc::new(Default::default()) }
    }

    fn provider_url(&self) -> Option<ProviderUrl> {
        if let Some(url) = self.infra.get_env_var("OPENAI_URL") {
            return Some(ProviderUrl::OpenAI(url));
        }

        if let Some(url) = self.infra.get_env_var("ANTHROPIC_URL") {
            return Some(ProviderUrl::Anthropic(url));
        }
        None
    }

    async fn resolve_provider(&self) -> Option<Provider> {
        // First, try to find the explicitly active provider
        if let Some(profile) = self
            .profile_service
            .get_active_profile()
            .await
            .ok()
            .flatten()
        {
            return Some(profile.provider);
        }
        // If no active provider, try to find the first one that can be configured
        // otherwise fallback to env provider
        let profiles = self
            .profile_service
            .list_profiles()
            .await
            .ok()
            .unwrap_or(vec![]);
        profiles
            .into_iter()
            .find_map(|p| p.provider.key().map(|_| p.provider.clone()))
            .or_else(|| resolve_env_provider(self.provider_url(), self.infra.as_ref()))
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra, P: ProfileService + Send + Sync> ProviderRegistry
    for ForgeProviderRegistry<F, P>
{
    async fn get_provider(&self) -> anyhow::Result<Provider> {
        if let Some(provider) = self.cache.read().await.as_ref() {
            return Ok(provider.clone());
        }

        let provider = self.resolve_provider().await.context("No valid provider configuration found. Please configure a profile in your `profiles.yaml` file or set one of the following environment variables: OPENROUTER_API_KEY, REQUESTY_API_KEY, XAI_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY.")?;
        self.cache.write().await.replace(provider.clone());
        Ok(provider)
    }

    async fn clear_provider_cache(&self) {
        *self.cache.write().await = None;
    }
}

fn resolve_env_provider<F: EnvironmentInfra>(
    url: Option<ProviderUrl>,
    env: &F,
) -> Option<Provider> {
    let keys: [ProviderSearch; 7] = [
        // ("FORGE_KEY", Box::new(Provider::forge)),
        ("OPENROUTER_API_KEY", Box::new(Provider::open_router)),
        ("REQUESTY_API_KEY", Box::new(Provider::requesty)),
        ("XAI_API_KEY", Box::new(Provider::xai)),
        ("OPENAI_API_KEY", Box::new(Provider::openai)),
        ("ANTHROPIC_API_KEY", Box::new(Provider::anthropic)),
        ("CEREBRAS_API_KEY", Box::new(Provider::cerebras)),
        ("ZAI_API_KEY", Box::new(Provider::zai)),
    ];

    keys.into_iter().find_map(|(key, fun)| {
        env.get_env_var(key).map(|key| {
            let provider = fun(&key);
            override_url(provider, url.clone())
        })
    })
}

fn override_url(mut provider: Provider, url: Option<ProviderUrl>) -> Provider {
    if let Some(url) = url {
        provider.url(url);
    }
    provider
}
