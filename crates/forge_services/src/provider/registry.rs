use std::sync::{Arc, OnceLock};

use forge_app::domain::{ModelId, Provider, ProviderId, ProviderResponse};
use forge_app::{AppConfigRepository, EnvironmentInfra, FileReaderInfra};
use forge_domain::ProviderRepository;
use handlebars::Handlebars;
use merge::Merge;
use serde::Deserialize;
use tokio::sync::OnceCell;
use url::Url;

use crate::ProviderError;
/// Represents the source of models for a provider
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Models {
    /// Models are fetched from a URL
    Url(String),
    /// Models are hardcoded in the configuration
    Hardcoded(Vec<forge_app::domain::Model>),
}

#[derive(Debug, Clone, Deserialize, Merge)]
struct ProviderConfig {
    #[merge(strategy = overwrite)]
    id: ProviderId,
    #[merge(strategy = overwrite)]
    api_key_vars: String,
    #[merge(strategy = merge::vec::append)]
    url_param_vars: Vec<String>,
    #[merge(strategy = overwrite)]
    response_type: ProviderResponse,
    #[merge(strategy = overwrite)]
    url: String,
    #[merge(strategy = overwrite)]
    models: Models,
}

fn overwrite<T>(base: &mut T, other: T) {
    *base = other;
}

/// Transparent wrapper for Vec<ProviderConfig> that implements custom merge
/// logic
#[derive(Debug, Clone, Deserialize, Merge)]
#[serde(transparent)]
struct ProviderConfigs(#[merge(strategy = merge_configs)] Vec<ProviderConfig>);

fn merge_configs(base: &mut Vec<ProviderConfig>, other: Vec<ProviderConfig>) {
    let mut map: std::collections::HashMap<_, _> = base.drain(..).map(|c| (c.id, c)).collect();

    for other_config in other {
        map.entry(other_config.id)
            .and_modify(|base_config| base_config.merge(other_config.clone()))
            .or_insert(other_config);
    }

    base.extend(map.into_values());
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
            .map_err(|e| anyhow::anyhow!("Failed to parse embedded provider configs: {e}"))
            .unwrap()
    })
}

pub struct ForgeProviderRegistry<F, R> {
    infra: Arc<F>,
    repo: Arc<R>,
    handlebars: &'static Handlebars<'static>,
    providers: OnceCell<Vec<Provider>>,
}

impl<F: EnvironmentInfra + FileReaderInfra, R: AppConfigRepository> ForgeProviderRegistry<F, R> {
    pub fn new(infra: Arc<F>, repo: Arc<R>) -> Self {
        Self {
            infra,
            repo,
            handlebars: get_handlebars(),
            providers: OnceCell::new(),
        }
    }

    /// Loads provider configs from the base directory if they exist
    async fn get_custom_provider_configs(&self) -> anyhow::Result<Vec<ProviderConfig>> {
        let environment = self.infra.get_environment();
        let provider_json_path = environment.base_path.join("provider.json");

        let json_str = self.infra.read_utf8(&provider_json_path).await?;
        let configs = serde_json::from_str(&json_str)?;
        Ok(configs)
    }

    async fn get_providers(&self) -> &Vec<Provider> {
        self.providers
            .get_or_init(|| async { self.init_providers().await })
            .await
    }

    async fn init_providers(&self) -> Vec<Provider> {
        let configs = self.get_merged_configs().await;

        let mut providers: Vec<Provider> = configs
            .into_iter()
            .filter_map(|config| {
                // Skip Forge provider as it's handled specially
                if config.id == ProviderId::Forge {
                    return None;
                }
                self.create_provider(&config).ok()
            })
            .collect();

        // Sort by ProviderId enum order to ensure deterministic, priority-based
        // ordering
        providers.sort_by(|a, b| a.id.cmp(&b.id));

        providers
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

        // Handle models based on the variant
        let models = match &config.models {
            Models::Url(model_url_template) => {
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
                forge_domain::Models::Url(model_url)
            }
            Models::Hardcoded(model_list) => forge_domain::Models::Hardcoded(model_list.clone()),
        };

        Ok(Provider {
            id: config.id,
            response: config.response_type.clone(),
            url: final_url,
            key: Some(api_key),
            models,
        })
    }

    async fn provider_from_id(&self, id: ProviderId) -> anyhow::Result<Provider> {
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
        let mut config = self.repo.get_app_config().await?;
        updater(&mut config);
        self.repo.set_app_config(&config).await?;
        Ok(())
    }

    /// Returns merged provider configs (embedded + custom)
    async fn get_merged_configs(&self) -> Vec<ProviderConfig> {
        let mut configs = ProviderConfigs(get_provider_configs().clone());
        // Merge custom configs into embedded configs
        configs.merge(ProviderConfigs(
            self.get_custom_provider_configs().await.unwrap_or_default(),
        ));

        configs.0
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + FileReaderInfra + Sync, R: AppConfigRepository> ProviderRepository
    for ForgeProviderRegistry<F, R>
{
    async fn get_default_provider(&self) -> anyhow::Result<Provider> {
        let app_config = self.repo.get_app_config().await?;
        if let Some(provider_id) = app_config.provider {
            return self.provider_from_id(provider_id).await;
        }

        // No active provider set, try to find the first available one
        self.get_first_available_provider().await
    }

    async fn set_default_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.update(|config| {
            config.provider = Some(provider_id);
        })
        .await
    }

    async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>> {
        Ok(self.get_providers().await.clone())
    }

    async fn get_default_model(&self, provider_id: &ProviderId) -> anyhow::Result<ModelId> {
        if let Some(model_id) = self.repo.get_app_config().await?.model.get(provider_id) {
            return Ok(model_id.clone());
        }

        // No active model set for the active provider, throw an error
        Err(forge_app::Error::NoActiveModel.into())
    }

    async fn set_default_model(
        &self,
        model: ModelId,
        provider_id: ProviderId,
    ) -> anyhow::Result<()> {
        self.update(|config| {
            config.model.insert(provider_id, model.clone());
        })
        .await
    }

    async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider> {
        self.provider_from_id(id).await
    }
}

#[cfg(test)]
mod tests {
    use forge_app::domain::ProviderResponse;
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

        // Check models exists and contains expected elements
        match &config.models {
            Models::Url(model_url) => {
                assert!(model_url.contains("api-version"));
                assert!(model_url.contains("/models"));
            }
            Models::Hardcoded(_) => panic!("Expected Models::Url variant"),
        }
    }
}

#[cfg(test)]
mod env_tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_app::domain::Environment;
    use pretty_assertions::assert_eq;

    use super::*;

    // Mock infrastructure that provides environment variables
    struct MockInfra {
        env_vars: HashMap<String, String>,
    }

    struct MockRepo;

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            Faker.fake()
        }

        fn get_env_var(&self, key: &str) -> Option<String> {
            self.env_vars.get(key).cloned()
        }
    }

    #[async_trait::async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read_utf8(&self, _path: &std::path::Path) -> anyhow::Result<String> {
            Err(anyhow::anyhow!("File not found"))
        }

        async fn read(&self, _path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
            Err(anyhow::anyhow!("File not found"))
        }

        async fn range_read_utf8(
            &self,
            _path: &std::path::Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_domain::FileInfo)> {
            Err(anyhow::anyhow!("File not found"))
        }
    }

    #[async_trait::async_trait]
    impl AppConfigRepository for MockRepo {
        async fn get_app_config(&self) -> anyhow::Result<forge_app::dto::AppConfig> {
            Ok(forge_app::dto::AppConfig::default())
        }

        async fn set_app_config(&self, _config: &forge_app::dto::AppConfig) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockRepo {
        async fn get_default_provider(&self) -> anyhow::Result<Provider> {
            Err(anyhow::anyhow!("No default provider configured"))
        }

        async fn set_default_provider(&self, _provider_id: ProviderId) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>> {
            Ok(vec![])
        }

        async fn get_default_model(&self, _provider_id: &ProviderId) -> anyhow::Result<ModelId> {
            Err(anyhow::anyhow!("No default model configured"))
        }

        async fn set_default_model(
            &self,
            _model: ModelId,
            _provider_id: ProviderId,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_provider(&self, _id: ProviderId) -> anyhow::Result<Provider> {
            Err(anyhow::anyhow!("Provider not found"))
        }
    }

    #[tokio::test]
    async fn test_create_azure_provider_with_handlebars_urls() {
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
        let repo = Arc::new(MockRepo);
        let registry = ForgeProviderRegistry::new(infra, repo);

        // Get Azure config from embedded configs
        let configs = get_provider_configs();
        let azure_config = configs
            .iter()
            .find(|c| c.id == ProviderId::Azure)
            .expect("Azure config should exist");

        // Create provider using the registry's test_create_provider method
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
        match provider.models {
            forge_domain::Models::Url(model_url) => {
                assert_eq!(
                    model_url.as_str(),
                    "https://my-test-resource.openai.azure.com/openai/models?api-version=2024-02-01-preview"
                );
            }
            forge_domain::Models::Hardcoded(_) => panic!("Expected Models::Url variant"),
        }
    }

    #[tokio::test]
    async fn test_custom_provider_urls() {
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
        let repo = Arc::new(MockRepo);
        let registry = ForgeProviderRegistry::new(infra, repo);
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

    #[tokio::test]
    async fn test_merge_base_provider_configs() {
        use std::io::Write;

        use tempfile::TempDir;

        // Create a temporary directory to act as base_path
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();

        // Create a custom provider.json in the base directory
        // Only override OpenAI, don't add custom providers
        let provider_json_path = base_path.join("provider.json");
        let mut file = std::fs::File::create(&provider_json_path).unwrap();
        let custom_config = r#"[
            {
                "id": "openai",
                "api_key_vars": "CUSTOM_OPENAI_KEY",
                "url_param_vars": [],
                "response_type": "OpenAI",
                "url": "https://custom.openai.com/v1/chat/completions",
                "models": "https://custom.openai.com/v1/models"
            }
        ]"#;
        file.write_all(custom_config.as_bytes()).unwrap();
        drop(file);

        // Create mock infra with the custom base_path
        let mut env_vars = HashMap::new();
        env_vars.insert("CUSTOM_OPENAI_KEY".to_string(), "test-key".to_string());

        struct CustomMockInfra {
            env_vars: HashMap<String, String>,
            base_path: PathBuf,
        }

        impl EnvironmentInfra for CustomMockInfra {
            fn get_environment(&self) -> Environment {
                use fake::{Fake, Faker};
                let mut env: Environment = Faker.fake();
                env.base_path = self.base_path.clone();
                env
            }

            fn get_env_var(&self, key: &str) -> Option<String> {
                self.env_vars.get(key).cloned()
            }
        }

        #[async_trait::async_trait]
        impl FileReaderInfra for CustomMockInfra {
            async fn read_utf8(&self, path: &std::path::Path) -> anyhow::Result<String> {
                tokio::fs::read_to_string(path).await.map_err(Into::into)
            }

            async fn read(&self, path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
                tokio::fs::read(path).await.map_err(Into::into)
            }

            async fn range_read_utf8(
                &self,
                _path: &std::path::Path,
                _start_line: u64,
                _end_line: u64,
            ) -> anyhow::Result<(String, forge_domain::FileInfo)> {
                Err(anyhow::anyhow!("Not implemented"))
            }
        }

        #[async_trait::async_trait]
        impl AppConfigRepository for CustomMockInfra {
            async fn get_app_config(&self) -> anyhow::Result<forge_app::dto::AppConfig> {
                Ok(forge_app::dto::AppConfig::default())
            }

            async fn set_app_config(
                &self,
                _config: &forge_app::dto::AppConfig,
            ) -> anyhow::Result<()> {
                Ok(())
            }
        }

        #[async_trait::async_trait]
        impl ProviderRepository for CustomMockInfra {
            async fn get_default_provider(&self) -> anyhow::Result<Provider> {
                Err(anyhow::anyhow!("No default provider configured"))
            }

            async fn set_default_provider(&self, _provider_id: ProviderId) -> anyhow::Result<()> {
                Ok(())
            }

            async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>> {
                Ok(vec![])
            }

            async fn get_default_model(
                &self,
                _provider_id: &ProviderId,
            ) -> anyhow::Result<ModelId> {
                Err(anyhow::anyhow!("No default model configured"))
            }

            async fn set_default_model(
                &self,
                _model: ModelId,
                _provider_id: ProviderId,
            ) -> anyhow::Result<()> {
                Ok(())
            }

            async fn get_provider(&self, _id: ProviderId) -> anyhow::Result<Provider> {
                Err(anyhow::anyhow!("Provider not found"))
            }
        }

        let infra = Arc::new(CustomMockInfra { env_vars, base_path });
        let repo = Arc::new(MockRepo);
        let registry = ForgeProviderRegistry::new(infra, repo);

        // Get merged configs
        let merged_configs = registry.get_merged_configs().await;

        // Verify OpenAI config was overridden
        let openai_config = merged_configs
            .iter()
            .find(|c| c.id == ProviderId::OpenAI)
            .expect("OpenAI config should exist");
        assert_eq!(openai_config.api_key_vars, "CUSTOM_OPENAI_KEY");
        assert_eq!(
            openai_config.url,
            "https://custom.openai.com/v1/chat/completions"
        );

        // Verify other embedded configs still exist
        let openrouter_config = merged_configs
            .iter()
            .find(|c| c.id == ProviderId::OpenRouter);
        assert!(openrouter_config.is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_get_first_available_provider_is_deterministic() {
        // Setup mock with multiple providers
        // Without sorting, the order from filter_map().collect() is non-deterministic
        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "openai-key".to_string());
        env_vars.insert("ANTHROPIC_API_KEY".to_string(), "anthropic-key".to_string());
        env_vars.insert(
            "OPENROUTER_API_KEY".to_string(),
            "openrouter-key".to_string(),
        );

        // Create multiple registry instances to expose non-deterministic behavior
        // Each instance will have a different provider order without sorting
        let mut all_first_providers = std::collections::HashSet::new();

        for _ in 0..100 {
            let infra = Arc::new(MockInfra { env_vars: env_vars.clone() });
            let repo = Arc::new(MockRepo);
            let registry = ForgeProviderRegistry::new(infra, repo);
            let first_provider = registry.get_first_available_provider().await.unwrap();
            all_first_providers.insert(first_provider.id);
        }

        // Without sorting, we might get different providers each time
        // With sorting, we should always get OpenAI
        // This assertion will FAIL without the fix
        assert_eq!(
            all_first_providers.len(),
            1,
            "get_first_available_provider should always return the same provider consistently across multiple registry instances, got: {:?}",
            all_first_providers
        );

        // Verify it's always OpenAI (first in ProviderId enum after Forge)
        let infra = Arc::new(MockInfra { env_vars });
        let repo = Arc::new(MockRepo);
        let registry = ForgeProviderRegistry::new(infra, repo);
        let first_provider = registry.get_first_available_provider().await.unwrap();
        assert_eq!(
            first_provider.id,
            ProviderId::OpenAI,
            "First provider should be OpenAI (first in ProviderId enum order after Forge)"
        );
    }
}
