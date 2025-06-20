use anyhow::Context;
use forge_app::ProviderRegistry;
use forge_domain::{ForgeConfig, Provider, ProviderUrl};
use std::sync::Arc;

use crate::EnvironmentInfra;

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

pub struct ForgeProviderRegistry<F> {
    infra: Arc<F>,
}

impl<F: EnvironmentInfra> ForgeProviderRegistry<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
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
    fn get_provider(&self, forge_config: ForgeConfig) -> Option<Provider> {
        if let Some(forge_key) = &forge_config.key_info {
            let provider = Provider::antinomy(forge_key.as_str());
            return Some(override_url(provider, self.provider_url()));
        }
        resolve_env_provider(self.provider_url(), self.infra.as_ref())
    }
}

impl<F: EnvironmentInfra> ProviderRegistry for ForgeProviderRegistry<F> {
    fn get_provider(&self, config: ForgeConfig) -> anyhow::Result<Provider> {
        self.get_provider(config)
            .context("Failed to resolve provider, maybe user is not logged in?")
    }

    fn provider_url(&self) -> anyhow::Result<ProviderUrl> {
        self.provider_url()
            .context("Failed to resolve provider URL")
    }
}

fn resolve_env_provider<F: EnvironmentInfra>(
    url: Option<ProviderUrl>,
    env: &F,
) -> Option<Provider> {
    let keys: [ProviderSearch; 4] = [
        ("FORGE_KEY", Box::new(Provider::antinomy)),
        ("OPENROUTER_API_KEY", Box::new(Provider::open_router)),
        ("OPENAI_API_KEY", Box::new(Provider::openai)),
        ("ANTHROPIC_API_KEY", Box::new(Provider::anthropic)),
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
