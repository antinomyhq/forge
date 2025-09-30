use std::sync::Arc;

use forge_app::ProviderRegistry;
use forge_app::dto::{
    ANTHROPIC_URL, CEREBRAS_URL, OPEN_ROUTER_URL, OPENAI_URL, Provider, ProviderId,
    ProviderResponse, REQUESTY_URL, XAI_URL, ZAI_CODING_URL, ZAI_URL,
};
use url::Url;

use crate::{AppConfigRepository, EnvironmentInfra, ProviderError};

pub struct ForgeProviderRegistry<F> {
    infra: Arc<F>,
}

impl<F: EnvironmentInfra + AppConfigRepository> ForgeProviderRegistry<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    fn provider_from_id(&self, id: forge_app::dto::ProviderId) -> anyhow::Result<Provider> {
        // First, match provider_id to get environment variable name and provider config
        let (env_var_name, api, url) = match id {
            ProviderId::OpenRouter => (
                "OPENROUTER_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(OPEN_ROUTER_URL).unwrap(),
            ),
            ProviderId::Requesty => (
                "REQUESTY_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(REQUESTY_URL).unwrap(),
            ),
            ProviderId::Xai => (
                "XAI_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(XAI_URL).unwrap(),
            ),
            ProviderId::OpenAI => (
                "OPENAI_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(OPENAI_URL).unwrap(),
            ),
            ProviderId::Anthropic => (
                "ANTHROPIC_API_KEY",
                ProviderResponse::Anthropic,
                Url::parse(ANTHROPIC_URL).unwrap(),
            ),
            ProviderId::Cerebras => (
                "CEREBRAS_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(CEREBRAS_URL).unwrap(),
            ),
            ProviderId::Zai => (
                "ZAI_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(ZAI_URL).unwrap(),
            ),
            ProviderId::ZaiCoding => (
                "ZAI_CODING_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(ZAI_CODING_URL).unwrap(),
            ),
            ProviderId::VertexAi => {
                if let Some(auth_token) = self.infra.get_env_var("VERTEX_AI_AUTH_TOKEN") {
                    return resolve_vertex_env_provider(&auth_token, self.infra.as_ref());
                } else {
                    return Err(ProviderError::env_var_not_found(
                        ProviderId::VertexAi,
                        "VERTEX_AI_AUTH_TOKEN",
                    )
                    .into());
                }
            }
            ProviderId::Forge => {
                // Forge provider isn't typically configured via env vars in the registry
                return Err(ProviderError::provider_not_available(ProviderId::Forge).into());
            }
        };

        // Get the API key and create provider using field assignment
        if let Some(api_key) = self.infra.get_env_var(env_var_name) {
            Ok(Provider { id, api, url, key: Some(api_key) })
        } else {
            Err(ProviderError::env_var_not_found(id, env_var_name).into())
        }
    }

    fn get_first_available_provider(&self) -> anyhow::Result<Provider> {
        // Define all provider IDs in order of preference
        let provider_ids = [
            ProviderId::OpenAI,
            ProviderId::Anthropic,
            ProviderId::OpenRouter,
            ProviderId::Xai,
            ProviderId::Cerebras,
            ProviderId::Zai,
            ProviderId::ZaiCoding,
            ProviderId::Requesty,
            ProviderId::VertexAi,
        ];

        for provider_id in provider_ids {
            if let Ok(provider) = self.provider_from_id(provider_id) {
                return Ok(provider);
            }
        }

        Err(forge_app::Error::NoActiveProvider.into())
    }

    fn provider_url(&self) -> Option<(ProviderResponse, Url)> {
        if let Some(url) = self.infra.get_env_var("OPENAI_URL")
            && let Ok(parsed_url) = Url::parse(&url)
        {
            return Some((ProviderResponse::OpenAI, parsed_url));
        }

        // Check for Anthropic URL override
        if let Some(url) = self.infra.get_env_var("ANTHROPIC_URL")
            && let Ok(parsed_url) = Url::parse(&url)
        {
            return Some((ProviderResponse::Anthropic, parsed_url));
        }
        None
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + AppConfigRepository> ProviderRegistry for ForgeProviderRegistry<F> {
    async fn get_active_provider(&self) -> anyhow::Result<Provider> {
        if let Some(app_config) = self.infra.get_app_config().await?
            && let Some(provider_id) = app_config.active_provider
        {
            let mut provider = self.provider_from_id(provider_id)?;

            // Apply URL overrides if present
            if let Some(provider_url) = self.provider_url() {
                provider = override_url(provider, Some(provider_url));
            }

            return Ok(provider);
        }

        // No active provider set, try to find the first available one
        let mut provider = self.get_first_available_provider()?;

        // Apply URL overrides if present
        if let Some(provider_url) = self.provider_url() {
            provider = override_url(provider, Some(provider_url));
        }

        Ok(provider)
    }

    async fn set_active_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        let mut app_config = self.infra.get_app_config().await?.unwrap_or_default();
        app_config.active_provider = Some(provider_id);
        self.infra.set_app_config(&app_config).await?;

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
    let project_id = env.get_env_var("PROJECT_ID").ok_or_else(|| {
        ProviderError::vertex_ai_config(
            "PROJECT_ID is missing. Please set the PROJECT_ID environment variable.",
        )
    })?;
    let location = env.get_env_var("LOCATION").ok_or_else(|| {
        ProviderError::vertex_ai_config(
            "LOCATION is missing. Please set the LOCATION environment variable.",
        )
    })?;
    Provider::vertex_ai(key, &project_id, &location)
}

fn override_url(provider: Provider, url_override: Option<(ProviderResponse, Url)>) -> Provider {
    if let Some((api, url)) = url_override {
        provider.api(api).url(url)
    } else {
        provider
    }
}
