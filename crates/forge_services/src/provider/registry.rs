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
    model_url: String,
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

        configs
            .iter()
            .filter_map(|config| {
                // Skip Forge provider as it's handled specially
                if config.id == ProviderId::Forge {
                    return None;
                }
                self.create_provider(config).ok()
            })
            .collect()
    }

    fn create_provider(&self, config: &ProviderConfig) -> anyhow::Result<Provider> {
        // Check API key environment variable
        let api_key = self
            .infra
            .get_env_var(&config.api_key_vars)
            .ok_or_else(|| ProviderError::env_var_not_found(config.id, &config.api_key_vars))?;

        // Check URL parameter environment variables and build template data
        // URL parameters are optional - only add them if they exist
        let mut template_data = std::collections::HashMap::new();

        for env_var in &config.url_param_vars {
            if let Some(value) = self.infra.get_env_var(env_var) {
                template_data.insert(env_var, value);
            } else if env_var == "OPENAI_URL" {
                template_data.insert(env_var, "https://api.openai.com/v1".to_string());
            } else if env_var == "ANTHROPIC_URL" {
                template_data.insert(env_var, "https://api.anthropic.com/v1".to_string());
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

        let final_url = Url::parse(&url)?;
        // Render optional model_url if present
        let model_url_template = &config.model_url;
        let model_url = Url::parse(
            &self
                .handlebars
                .render_template(model_url_template, &template_data)
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to render model_url template for {}: {}",
                        config.id,
                        e
                    )
                })?,
        )?;

        Ok(Provider {
            id: config.id,
            response: config.response_type.clone(),
            url: final_url,
            key: Some(api_key),
            model_url,
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
        assert_eq!(
            openrouter_config.url,
            "https://openrouter.ai/api/v1/chat/completions"
        );
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
        assert_eq!(config.url, "https://openrouter.ai/api/v1/chat/completions");
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
        let template = "{{#if (eq LOCATION \"global\")}}https://aiplatform.googleapis.com/v1/projects/{{PROJECT_ID}}/locations/{{LOCATION}}/endpoints/openapi/{{else}}https://{{LOCATION}}-aiplatform.googleapis.com/v1/projects/{{PROJECT_ID}}/locations/{{LOCATION}}/endpoints/openapi/{{/if}}";

        let mut data = std::collections::HashMap::new();
        data.insert("PROJECT_ID".to_string(), "test-project".to_string());
        data.insert("LOCATION".to_string(), "global".to_string());

        let result = handlebars.render_template(template, &data).unwrap();
        assert_eq!(
            result,
            "https://aiplatform.googleapis.com/v1/projects/test-project/locations/global/endpoints/openapi/"
        );

        data.insert("LOCATION".to_string(), "us-central1".to_string());
        let result = handlebars.render_template(template, &data).unwrap();
        assert_eq!(
            result,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/test-project/locations/us-central1/endpoints/openapi/"
        );
    }

    #[test]
    fn test_azure_config() {
        let configs = get_provider_configs();
        let config = configs.iter().find(|c| c.id == ProviderId::Azure).unwrap();
        assert_eq!(config.id, ProviderId::Azure);
        assert_eq!(config.api_key_vars, "AZURE_API_KEY");
        assert_eq!(
            config.url_param_vars,
            vec![
                "AZURE_RESOURCE_NAME".to_string(),
                "AZURE_DEPLOYMENT_NAME".to_string(),
                "AZURE_API_VERSION".to_string()
            ]
        );
        assert_eq!(config.response_type, ProviderResponse::OpenAI);

        // Check URL (now contains full chat completion URL)
        assert!(config.url.contains("{{"));
        assert!(config.url.contains("}}"));
        assert!(config.url.contains("openai.azure.com"));
        assert!(config.url.contains("api-version"));
        assert!(config.url.contains("deployments"));
        assert!(config.url.contains("chat/completions"));

        // Check model_url exists and contains expected elements
        let model_url = config.model_url.clone();
        assert!(model_url.contains("api-version"));
        assert!(model_url.contains("/models"));
    }

    #[test]
    fn test_azure_url_rendering() {
        let handlebars = Handlebars::new();
        let mut data = std::collections::HashMap::new();
        data.insert("AZURE_RESOURCE_NAME".to_string(), "my-resource".to_string());
        data.insert("AZURE_DEPLOYMENT_NAME".to_string(), "gpt-4".to_string());
        data.insert(
            "AZURE_API_VERSION".to_string(),
            "2024-02-15-preview".to_string(),
        );

        // Test base URL
        let base_template = "https://{{AZURE_RESOURCE_NAME}}.openai.azure.com/openai/";
        let base_result = handlebars.render_template(base_template, &data).unwrap();
        assert_eq!(base_result, "https://my-resource.openai.azure.com/openai/");

        // Test chat completion URL
        let chat_template = "https://{{AZURE_RESOURCE_NAME}}.openai.azure.com/openai/deployments/{{AZURE_DEPLOYMENT_NAME}}/chat/completions?api-version={{AZURE_API_VERSION}}";
        let chat_result = handlebars.render_template(chat_template, &data).unwrap();
        assert_eq!(
            chat_result,
            "https://my-resource.openai.azure.com/openai/deployments/gpt-4/chat/completions?api-version=2024-02-15-preview"
        );

        // Test model URL
        let model_template = "https://{{AZURE_RESOURCE_NAME}}.openai.azure.com/openai/models?api-version={{AZURE_API_VERSION}}";
        let model_result = handlebars.render_template(model_template, &data).unwrap();
        assert_eq!(
            model_result,
            "https://my-resource.openai.azure.com/openai/models?api-version=2024-02-15-preview"
        );
    }
}

#[cfg(test)]
mod env_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use forge_app::domain::Environment;
    use pretty_assertions::assert_eq;

    use super::*;

    // Mock infrastructure that provides environment variables
    struct MockInfra {
        env_vars: HashMap<String, String>,
    }

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            // Return a minimal Environment for testing
            Environment {
                os: "test".to_string(),
                pid: 1,
                cwd: std::path::PathBuf::from("/test"),
                home: None,
                shell: "test".to_string(),
                base_path: std::path::PathBuf::from("/test"),
                forge_api_url: Url::parse("https://test.com").unwrap(),
                retry_config: Default::default(),
                max_search_lines: 100,
                max_search_result_bytes: 1000,
                fetch_truncation_limit: 1000,
                stdout_max_prefix_length: 100,
                stdout_max_suffix_length: 100,
                stdout_max_line_length: 500,
                max_read_size: 2000,
                http: Default::default(),
                max_file_size: 100000,
                tool_timeout: 300,
                auto_open_dump: false,
                custom_history_path: None,
                max_conversations: 100,
            }
        }

        fn get_env_var(&self, key: &str) -> Option<String> {
            self.env_vars.get(key).cloned()
        }
    }

    #[async_trait::async_trait]
    impl AppConfigRepository for MockInfra {
        async fn get_app_config(&self) -> anyhow::Result<forge_app::dto::AppConfig> {
            Ok(forge_app::dto::AppConfig::default())
        }

        async fn set_app_config(&self, _config: &forge_app::dto::AppConfig) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_create_azure_provider_with_handlebars_urls() {
        // Setup environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert("AZURE_API_KEY".to_string(), "test-key-123".to_string());
        env_vars.insert(
            "AZURE_RESOURCE_NAME".to_string(),
            "my-test-resource".to_string(),
        );
        env_vars.insert(
            "AZURE_DEPLOYMENT_NAME".to_string(),
            "gpt-4-deployment".to_string(),
        );
        env_vars.insert(
            "AZURE_API_VERSION".to_string(),
            "2024-02-01-preview".to_string(),
        );

        let infra = Arc::new(MockInfra { env_vars });
        let registry = ForgeProviderRegistry::new(infra);

        // Get Azure config
        let configs = get_provider_configs();
        let azure_config = configs
            .iter()
            .find(|c| c.id == ProviderId::Azure)
            .expect("Azure config should exist");

        // Create provider using the registry's create_provider method
        let provider = registry
            .create_provider(azure_config)
            .expect("Should create Azure provider");

        // Verify all URLs are correctly rendered
        assert_eq!(provider.id, ProviderId::Azure);
        assert_eq!(provider.key, Some("test-key-123".to_string()));

        // Check chat completion URL (url field now contains the chat completion URL)
        let chat_url = provider.url;
        assert_eq!(
            chat_url.as_str(),
            "https://my-test-resource.openai.azure.com/openai/deployments/gpt-4-deployment/chat/completions?api-version=2024-02-01-preview"
        );

        // Check model URL
        let model_url = provider.model_url;
        assert_eq!(
            model_url.as_str(),
            "https://my-test-resource.openai.azure.com/openai/models?api-version=2024-02-01-preview"
        );
    }

    #[tokio::test]
    async fn test_custom_anthropic_provider_with_env_var() {
        let mut env_vars = HashMap::new();
        env_vars.insert("ANTHROPIC_API_KEY".to_string(), "test-key".to_string());
        env_vars.insert(
            "ANTHROPIC_URL".to_string(),
            "https://custom.anthropic.com/v1".to_string(),
        );

        let infra = Arc::new(MockInfra { env_vars });
        let registry = ForgeProviderRegistry::new(infra);
        let provider = registry
            .provider_from_id(ProviderId::Anthropic)
            .await
            .unwrap();

        assert_eq!(
            provider.url.as_str(),
            "https://custom.anthropic.com/v1/messages"
        );
        assert_eq!(
            provider.model_url.as_str(),
            "https://custom.anthropic.com/v1/models"
        );
    }

    #[tokio::test]
    async fn test_openai_no_custom_url() {
        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

        let infra = Arc::new(MockInfra { env_vars });
        let registry = ForgeProviderRegistry::new(infra);
        let providers = registry.get_all_providers().await.unwrap();
        let openai_provider = providers
            .iter()
            .find(|p| p.id == ProviderId::OpenAI)
            .unwrap();
        assert_eq!(
            openai_provider.url.as_str(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            openai_provider.model_url.as_str(),
            "https://api.openai.com/v1/models"
        );

        let anthropic_provider = providers.iter().find(|p| p.id == ProviderId::Anthropic);
        assert!(anthropic_provider.is_none());
    }

    #[tokio::test]
    async fn test_all_custom_providers_with_env_vars() {
        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());
        env_vars.insert(
            "OPENAI_URL".to_string(),
            "https://custom.openai.com/v1".to_string(),
        );
        env_vars.insert("ANTHROPIC_API_KEY".to_string(), "test-key".to_string());
        env_vars.insert(
            "ANTHROPIC_URL".to_string(),
            "https://custom.anthropic.com/v1".to_string(),
        );

        let infra = Arc::new(MockInfra { env_vars });
        let registry = ForgeProviderRegistry::new(infra);
        let providers = registry.get_all_providers().await.unwrap();

        let openai_provider = providers
            .iter()
            .find(|p| p.id == ProviderId::OpenAI)
            .unwrap();
        let anthropic_provider = providers
            .iter()
            .find(|p| p.id == ProviderId::Anthropic)
            .unwrap();

        assert_eq!(
            openai_provider.url.as_str(),
            "https://custom.openai.com/v1/chat/completions"
        );
        assert_eq!(
            anthropic_provider.url.as_str(),
            "https://custom.anthropic.com/v1/messages"
        );
    }
}
