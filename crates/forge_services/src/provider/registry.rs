use std::sync::{Arc, OnceLock};

use forge_app::ProviderRegistry;
use forge_app::domain::{AgentId, ModelId};
use forge_app::dto::{Provider, ProviderId, ProviderResponse};
use handlebars::Handlebars;
use serde::Deserialize;
use tokio::sync::OnceCell;
use url::Url;

use crate::{AppConfigRepository, EnvironmentInfra, ProviderError};

#[derive(Debug, Deserialize)]
struct ProviderConfig {
    id: ProviderId,
    api_key_vars: String,
    url_param_vars: Vec<String>,
    response_type: ProviderResponse,
    url: String,
}

static HANDLEBARS: OnceLock<Handlebars<'static>> = OnceLock::new();
static PROVIDER_CONFIGS: OnceLock<Vec<ProviderConfig>> = OnceLock::new();

fn get_handlebars() -> &'static Handlebars<'static> {
    HANDLEBARS.get_or_init(Handlebars::new)
}

fn get_provider_configs() -> &'static Vec<ProviderConfig> {
    PROVIDER_CONFIGS.get_or_init(|| {
        let json_str = include_str!("provider.json");
        serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse provider configs: {}", e))
            .unwrap()
    })
}

pub struct ForgeProviderRegistry<F> {
    infra: Arc<F>,
    handlebars: &'static Handlebars<'static>,
    providers: OnceCell<Vec<Provider>>,
}

impl<F: EnvironmentInfra + AppConfigRepository> ForgeProviderRegistry<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            infra,
            handlebars: get_handlebars(),
            providers: OnceCell::new(),
        }
    }

    async fn get_providers(&self) -> &Vec<Provider> {
        self.providers
            .get_or_init(|| async { self.init_providers() })
            .await
    }

    fn init_providers(&self) -> Vec<Provider> {
        let configs = get_provider_configs();
        let provider_url_override = self.provider_url();

        configs
            .iter()
            .filter_map(|config| {
                // Skip Forge provider as it's handled specially
                if config.id == ProviderId::Forge {
                    return None;
                }
                self.create_provider(config, provider_url_override.clone())
                    .ok()
            })
            .collect()
    }

    fn create_provider(
        &self,
        config: &ProviderConfig,
        provider_url_override: Option<(ProviderResponse, Url)>,
    ) -> anyhow::Result<Provider> {
        // Check API key environment variable
        let api_key = self
            .infra
            .get_env_var(&config.api_key_vars)
            .ok_or_else(|| ProviderError::env_var_not_found(config.id, &config.api_key_vars))?;

        // Check URL parameter environment variables and build template data
        let mut template_data = std::collections::HashMap::new();
        for env_var in &config.url_param_vars {
            if let Some(value) = self.infra.get_env_var(env_var) {
                // Convert env var names to handlebars-friendly variable names
                let key_name = env_var.to_lowercase().replace('_', "");
                template_data.insert(key_name, value);
            } else {
                return Err(ProviderError::env_var_not_found(config.id, env_var).into());
            }
        }

        // Render URL using handlebars
        let url = self
            .handlebars
            .render_template(&config.url, &template_data)
            .map_err(|e| {
                anyhow::anyhow!("Failed to render URL template for {}: {}", config.id, e)
            })?;

        // Handle URL overrides for OpenAI and Anthropic (preserve existing behavior)
        let final_url = match config.id {
            ProviderId::OpenAI => {
                if let Some((ProviderResponse::OpenAI, override_url)) = provider_url_override {
                    override_url
                } else {
                    Url::parse(&url)?
                }
            }
            ProviderId::Anthropic => {
                if let Some((ProviderResponse::Anthropic, override_url)) = provider_url_override {
                    override_url
                } else {
                    Url::parse(&url)?
                }
            }
            _ => Url::parse(&url)?,
        };

        Ok(Provider {
            id: config.id,
            response: config.response_type.clone(),
            url: final_url,
            key: Some(api_key),
        })
    }

    async fn provider_from_id(&self, id: forge_app::dto::ProviderId) -> anyhow::Result<Provider> {
        // Handle special cases first
        if id == ProviderId::Forge {
            // Forge provider isn't typically configured via env vars in the registry
            return Err(ProviderError::provider_not_available(ProviderId::Forge).into());
        }

        // Look up provider from cached providers
        self.get_providers()
            .await
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| ProviderError::provider_not_available(id).into())
    }

    async fn get_first_available_provider(&self) -> anyhow::Result<Provider> {
        self.get_providers()
            .await
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
        if let Some(provider_id) = app_config.provider {
            return self.provider_from_id(provider_id).await;
        }

        // No active provider set, try to find the first available one
        self.get_first_available_provider().await
    }

    async fn set_active_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.update(|config| {
            config.provider = Some(provider_id);
        })
        .await
    }

    async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>> {
        Ok(self.get_providers().await.clone())
    }

    async fn get_active_model(&self) -> anyhow::Result<ModelId> {
        let provider_id = self.get_active_provider().await?.id;

        if let Some(model_id) = self.infra.get_app_config().await?.model.get(&provider_id) {
            return Ok(model_id.clone());
        }

        // No active model set for the active provider, throw an error
        Err(forge_app::Error::NoActiveModel.into())
    }

    async fn set_active_model(&self, model: ModelId) -> anyhow::Result<()> {
        let provider_id = self.get_active_provider().await?.id;
        self.update(|config| {
            config.model.insert(provider_id, model.clone());
        })
        .await
    }

    async fn get_active_agent(&self) -> anyhow::Result<Option<AgentId>> {
        let app_config = self.infra.get_app_config().await?;
        Ok(app_config.agent)
    }

    async fn set_active_agent(&self, agent_id: AgentId) -> anyhow::Result<()> {
        self.update(|config| {
            config.agent = Some(agent_id);
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_load_provider_configs() {
        let configs = get_provider_configs();
        assert!(!configs.is_empty());

        // Test that OpenRouter config is loaded correctly
        let openrouter_config = configs
            .iter()
            .find(|c| c.id == ProviderId::OpenRouter)
            .unwrap();
        assert_eq!(openrouter_config.api_key_vars, "OPENROUTER_API_KEY");
        assert_eq!(openrouter_config.url_param_vars, Vec::<String>::new());
        assert_eq!(openrouter_config.response_type, ProviderResponse::OpenAI);
        assert_eq!(openrouter_config.url, "https://openrouter.ai/api/v1/");
    }

    #[test]
    fn test_find_provider_config() {
        let configs = get_provider_configs();
        let config = configs
            .iter()
            .find(|c| c.id == ProviderId::OpenRouter)
            .unwrap();
        assert_eq!(config.id, ProviderId::OpenRouter);
        assert_eq!(config.api_key_vars, "OPENROUTER_API_KEY");
        assert_eq!(config.url_param_vars, Vec::<String>::new());
        assert_eq!(config.response_type, ProviderResponse::OpenAI);
        assert_eq!(config.url, "https://openrouter.ai/api/v1/");
    }

    #[test]
    fn test_vertex_ai_config() {
        let configs = get_provider_configs();
        let config = configs
            .iter()
            .find(|c| c.id == ProviderId::VertexAi)
            .unwrap();
        assert_eq!(config.id, ProviderId::VertexAi);
        assert_eq!(config.api_key_vars, "VERTEX_AI_AUTH_TOKEN");
        assert_eq!(
            config.url_param_vars,
            vec!["PROJECT_ID".to_string(), "LOCATION".to_string()]
        );
        assert_eq!(config.response_type, ProviderResponse::OpenAI);
        assert!(config.url.contains("{{"));
        assert!(config.url.contains("}}"));
    }

    #[test]
    fn test_handlebars_url_rendering() {
        let handlebars = Handlebars::new();
        let template = "{{#if (eq location \"global\")}}https://aiplatform.googleapis.com/v1/projects/{{project_id}}/locations/{{location}}/endpoints/openapi/{{else}}https://{{location}}-aiplatform.googleapis.com/v1/projects/{{project_id}}/locations/{{location}}/endpoints/openapi/{{/if}}";

        let mut data = std::collections::HashMap::new();
        data.insert("project_id".to_string(), "test-project".to_string());
        data.insert("location".to_string(), "global".to_string());

        let result = handlebars.render_template(template, &data).unwrap();
        assert_eq!(
            result,
            "https://aiplatform.googleapis.com/v1/projects/test-project/locations/global/endpoints/openapi/"
        );

        data.insert("location".to_string(), "us-central1".to_string());
        let result = handlebars.render_template(template, &data).unwrap();
        assert_eq!(
            result,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/test-project/locations/us-central1/endpoints/openapi/"
        );
    }
}
