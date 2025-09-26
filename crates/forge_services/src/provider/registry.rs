use std::sync::Arc;

use forge_app::ProviderRegistry;
use forge_app::dto::{Provider, ProviderUrl};
use tokio::sync::RwLock;
use url::Url;

use crate::EnvironmentInfra;


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
        if let Some(url) = self.infra.get_env_var("OPENAI_URL")
            && let Ok(parsed_url) = Url::parse(&url)
        {
            return Some(ProviderUrl::OpenAI(parsed_url));
        }

        // Check for Anthropic URL override
        if let Some(url) = self.infra.get_env_var("ANTHROPIC_URL")
            && let Ok(parsed_url) = Url::parse(&url)
        {
            return Some(ProviderUrl::Anthropic(parsed_url));
        }
        None
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra> ProviderRegistry for ForgeProviderRegistry<F> {
    async fn get_active_provider(&self) -> anyhow::Result<Provider> {
        self.cache
            .read()
            .await
            .as_ref()
            .cloned()
            .ok_or_else(|| forge_app::Error::NoActiveProvider.into())
    }

    async fn set_active_provider(&self, provider: Provider) -> anyhow::Result<()> {
        self.cache.write().await.replace(provider);
        Ok(())
    }

    async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>> {
        let mut providers = Vec::new();
        let url = self.provider_url();

        // Get all available providers based on environment variables
        let keys: [(&str, Box<dyn Fn(&str) -> Provider>); 8] = [
            ("OPENROUTER_API_KEY", Box::new(Provider::open_router)),
            ("REQUESTY_API_KEY", Box::new(Provider::requesty)),
            ("XAI_API_KEY", Box::new(Provider::xai)),
            ("OPENAI_API_KEY", Box::new(Provider::openai)),
            ("ANTHROPIC_API_KEY", Box::new(Provider::anthropic)),
            ("CEREBRAS_API_KEY", Box::new(Provider::cerebras)),
            ("ZAI_API_KEY", Box::new(Provider::zai)),
            ("ZAI_CODING_API_KEY", Box::new(Provider::zai_coding)),
        ];

        for (key, provider_fn) in keys.iter() {
            if let Some(api_key) = self.infra.get_env_var(key) {
                let provider = provider_fn(&api_key);
                providers.push(override_url(provider, url.clone()));
            }
        }

        // Check for Vertex AI
        if let Some(auth_token) = self.infra.get_env_var("VERTEX_AI_AUTH_TOKEN")
            && let Ok(provider) = resolve_vertex_env_provider(&auth_token, self.infra.as_ref())
        {
            providers.push(provider);
        }

        Ok(providers)
    }
}

fn resolve_vertex_env_provider<F: EnvironmentInfra>(
    key: &str,
    env: &F,
) -> anyhow::Result<Provider> {
    let project_id = env.get_env_var("PROJECT_ID").ok_or(anyhow::anyhow!(
        "PROJECT_ID is missing. Please set the PROJECT_ID environment variable."
    ))?;
    let location = env.get_env_var("LOCATION").ok_or(anyhow::anyhow!(
        "LOCATION is missing. Please set the LOCATION environment variable."
    ))?;
    Provider::vertex_ai(key, &project_id, &location)
}

fn override_url(provider: Provider, url: Option<ProviderUrl>) -> Provider {
    if let Some(url) = url {
        provider.url(url)
    } else {
        provider
    }
}
