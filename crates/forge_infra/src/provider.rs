use forge_domain::{ForgeKey, Provider, ProviderUrl};
use forge_services::{EnvironmentInfra, ProviderInfra};
use std::sync::Arc;

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

#[derive(Clone)]
pub struct ForgeProviderInfra<F>(Arc<F>);

impl<F> ForgeProviderInfra<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

impl<F: EnvironmentInfra> ProviderInfra for ForgeProviderInfra<F> {
    fn get_provider_infra(&self, forge_key: Option<ForgeKey>) -> Option<Provider> {
        if let Some(forge_key) = forge_key {
            let provider = Provider::antinomy(forge_key.as_str());
            return Some(override_url(provider, self.provider_url()));
        }
        resolve_env_provider(self.provider_url(), self.0.as_ref())
    }

    fn provider_url(&self) -> Option<ProviderUrl> {
        if let Ok(url) = std::env::var("OPENAI_URL") {
            return Some(ProviderUrl::OpenAI(url));
        }

        // Check for Anthropic URL override
        if let Ok(url) = std::env::var("ANTHROPIC_URL") {
            return Some(ProviderUrl::Anthropic(url));
        }
        None
    }
}

// For backwards compatibility
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
