use std::sync::Arc;

use anyhow::Context;
use forge_app::ProviderRegistry;
use forge_app::domain::{Provider, ProviderUrl};
use forge_app::dto::Profile;
use tokio::sync::RwLock;

use crate::EnvironmentInfra;

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

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

    fn provider_url(&self) -> Option<ProviderUrl> {
        if let Some(url) = self.infra.get_env_var("OPENAI_URL") {
            return Some(ProviderUrl::OpenAI(url));
        }

        // Check for Anthropic URL override
        if let Some(url) = self.infra.get_env_var("ANTHROPIC_URL") {
            return Some(ProviderUrl::Anthropic(url));
        }
        None
    }

    fn resolve_provider(&self, profile: &Profile) -> Option<Provider> {
        // Use the provided profile's provider if it's valid
        if profile.provider.key().is_some() {
            return Some(profile.provider.clone());
        }

        // Fallback to environment variables
        resolve_env_provider(self.provider_url(), self.infra.as_ref())
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra> ProviderRegistry for ForgeProviderRegistry<F> {
    async fn get_provider(&self, profile: &Profile) -> anyhow::Result<Provider> {
        if let Some(provider) = self.cache.read().await.as_ref() {
            return Ok(provider.clone());
        }

        let provider = self.resolve_provider(profile).context("No valid provider configuration found. Please configure a profile in your `profiles.yaml` file or set one of the following environment variables: OPENROUTER_API_KEY, REQUESTY_API_KEY, XAI_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY.")?;
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
    let keys: [ProviderSearch; 8] = [
        // ("FORGE_KEY", Box::new(Provider::forge)),
        ("OPENROUTER_API_KEY", Box::new(Provider::open_router)),
        ("REQUESTY_API_KEY", Box::new(Provider::requesty)),
        ("XAI_API_KEY", Box::new(Provider::xai)),
        ("OPENAI_API_KEY", Box::new(Provider::openai)),
        ("ANTHROPIC_API_KEY", Box::new(Provider::anthropic)),
        ("CEREBRAS_API_KEY", Box::new(Provider::cerebras)),
        ("ZAI_API_KEY", Box::new(Provider::zai)),
        ("ZAI_CODING_API_KEY", Box::new(Provider::zai_coding)),
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
