use std::sync::Arc;

use forge_app::ProviderRegistry;
use forge_app::domain::{AgentId, ModelId};
use forge_app::dto::{Provider, ProviderId, ProviderResponse};
use strum::IntoEnumIterator;
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
                Url::parse(Provider::OPEN_ROUTER_URL).unwrap(),
            ),
            ProviderId::Requesty => (
                "REQUESTY_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(Provider::REQUESTY_URL).unwrap(),
            ),
            ProviderId::Xai => (
                "XAI_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(Provider::XAI_URL).unwrap(),
            ),
            ProviderId::OpenAI => {
                let url = match self.provider_url() {
                    Some((ProviderResponse::OpenAI, url)) => url,
                    _ => Url::parse(Provider::OPENAI_URL).unwrap(),
                };

                ("OPENAI_API_KEY", ProviderResponse::OpenAI, url)
            }
            ProviderId::Anthropic => {
                let url = match self.provider_url() {
                    Some((ProviderResponse::Anthropic, url)) => url,
                    _ => Url::parse(Provider::ANTHROPIC_URL).unwrap(),
                };

                ("ANTHROPIC_API_KEY", ProviderResponse::Anthropic, url)
            }
            ProviderId::Cerebras => (
                "CEREBRAS_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(Provider::CEREBRAS_URL).unwrap(),
            ),
            ProviderId::Zai => (
                "ZAI_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(Provider::ZAI_URL).unwrap(),
            ),
            ProviderId::ZaiCoding => (
                "ZAI_CODING_API_KEY",
                ProviderResponse::OpenAI,
                Url::parse(Provider::ZAI_CODING_URL).unwrap(),
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
            Ok(Provider { id, response: api, url, key: Some(api_key) })
        } else {
            Err(ProviderError::env_var_not_found(id, env_var_name).into())
        }
    }

    async fn get_first_available_provider(&self) -> anyhow::Result<Provider> {
        self.get_all_providers()
            .await?
            .first()
            .cloned()
            .ok_or_else(|| forge_app::Error::NoActiveProvider.into())
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

    async fn update<U>(&self, updater: U) -> anyhow::Result<()>
    where
        U: FnOnce(&mut forge_app::dto::AppConfig),
    {
        let mut config = self.infra.get_app_config().await?;
        updater(&mut config);
        self.infra.set_app_config(&config).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + AppConfigRepository> ProviderRegistry for ForgeProviderRegistry<F> {
    async fn get_active_provider(&self) -> anyhow::Result<Provider> {
        let app_config = self.infra.get_app_config().await?;
        if let Some(provider_id) = app_config.active_provider {
            return self.provider_from_id(provider_id);
        }

        // No active provider set, try to find the first available one
        self.get_first_available_provider().await
    }

    async fn set_active_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.update(|config| {
            config.active_provider = Some(provider_id);
        })
        .await
    }

    async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>> {
        // Define all provider IDs in order of preference

        let mut providers = ProviderId::iter().collect::<Vec<_>>();
        providers.sort();
        Ok(providers
            .iter()
            .filter_map(|id| self.provider_from_id(*id).ok())
            .collect::<Vec<_>>())
    }

    async fn get_active_model(&self) -> anyhow::Result<ModelId> {
        let app_config = self.infra.get_app_config().await?;
        if let Some(model_id) = app_config.active_model {
            return Ok(model_id);
        }

        // No active model set, throw an error
        Err(forge_app::Error::NoActiveModel.into())
    }

    async fn set_active_model(&self, model: ModelId) -> anyhow::Result<()> {
        self.update(|config| {
            config.active_model = Some(model.clone());
        })
        .await
    }

    async fn get_active_agent(&self) -> anyhow::Result<Option<AgentId>> {
        let app_config = self.infra.get_app_config().await?;
        Ok(app_config.active_agent)
    }

    async fn set_active_agent(&self, agent_id: AgentId) -> anyhow::Result<()> {
        self.update(|config| {
            config.active_agent = Some(agent_id);
        })
        .await
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
