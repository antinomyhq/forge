use std::sync::{Arc, OnceLock};

use forge_app::ProviderRegistry;
use forge_app::domain::{AgentId, ModelId};
use forge_app::dto::{Provider, ProviderId, ProviderResponse, URLParam};
use handlebars::Handlebars;
use merge::Merge;
use serde::Deserialize;
use tokio::sync::OnceCell;
use url::Url;

use crate::{
    AppConfigRepository, EnvironmentInfra, FileReaderInfra, ProviderCredentialRepository,
    ProviderError,
};

/// Represents the source of models for a provider
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum Models {
    /// Models are fetched from a URL
    Url(String),
    /// Models are hardcoded in the configuration
    Hardcoded(Vec<forge_app::domain::Model>),
}

#[derive(Debug, Clone, Deserialize, Merge)]
pub(crate) struct ProviderConfig {
    #[merge(strategy = overwrite)]
    pub id: ProviderId,
    #[merge(strategy = overwrite)]
    pub api_key_vars: String,
    #[merge(strategy = merge::vec::append)]
    pub url_param_vars: Vec<URLParam>,
    #[merge(strategy = overwrite)]
    pub response_type: ProviderResponse,
    #[merge(strategy = overwrite)]
    pub url: String,
    #[merge(strategy = overwrite)]
    pub models: Models,
    #[merge(strategy = overwrite)]
    pub auth_methods: Vec<crate::provider::AuthMethod>,
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

pub(crate) fn get_provider_configs() -> &'static Vec<ProviderConfig> {
    PROVIDER_CONFIGS.get_or_init(|| {
        let json_str = include_str!("provider.json");
        serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse embedded provider configs: {e}"))
            .unwrap()
    })
}

pub(crate) fn get_provider_config(provider_id: &ProviderId) -> Option<&'static ProviderConfig> {
    get_provider_configs()
        .iter()
        .find(|config| &config.id == provider_id)
}

/// Get credential variable names for a provider (for migration purposes)
pub fn get_provider_credential_vars(provider_id: &ProviderId) -> Option<(String, Vec<URLParam>)> {
    get_provider_config(provider_id)
        .map(|config| (config.api_key_vars.clone(), config.url_param_vars.to_vec()))
}

/// Get auth methods for a provider
pub fn get_provider_auth_methods(provider_id: &ProviderId) -> Vec<crate::provider::AuthMethod> {
    get_provider_config(provider_id)
        .map(|config| config.auth_methods.clone())
        .unwrap_or_else(|| {
            // Fallback for custom providers
            vec![crate::provider::AuthMethod::ApiKey]
        })
}

/// Get environment variable names for a provider
pub fn get_provider_env_vars(provider_id: &ProviderId) -> Vec<String> {
    get_provider_config(provider_id)
        .and_then(|config| {
            let key = config.api_key_vars.trim();
            if key.is_empty() {
                None
            } else {
                Some(vec![key.to_string()])
            }
        })
        .unwrap_or_default()
}

pub struct ForgeProviderRegistry<F> {
    infra: Arc<F>,
    handlebars: &'static Handlebars<'static>,
    providers: OnceCell<Vec<Provider>>,
}

impl<
    F: EnvironmentInfra
        + AppConfigRepository
        + FileReaderInfra
        + ProviderCredentialRepository
        + 'static,
> ForgeProviderRegistry<F>
{
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            infra,
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

        configs
            .into_iter()
            .filter_map(|config| {
                // Skip Forge provider as it's handled specially
                if config.id == ProviderId::Forge {
                    return None;
                }
                // Note: This is synchronous initialization, only loads env-based providers
                // For database credentials, use provider_from_id() which is async
                self.create_provider_from_env(&config).ok()
            })
            .collect()
    }

    fn create_provider_from_env(&self, config: &ProviderConfig) -> anyhow::Result<Provider> {
        // Check API key environment variable
        let api_key = self
            .infra
            .get_env_var(&config.api_key_vars)
            .ok_or_else(|| ProviderError::env_var_not_found(config.id, &config.api_key_vars))?
            .into();

        // Check URL parameter environment variables and build template data
        // URL parameters are optional - only add them if they exist
        let mut template_data: std::collections::HashMap<&str, String> =
            std::collections::HashMap::new();

        for env_var in &config.url_param_vars {
            if let Some(value) = self.infra.get_env_var(env_var.as_ref()) {
                template_data.insert(env_var.as_ref(), value);
            } else if env_var == &URLParam::from("OPENAI_URL".to_owned()) {
                template_data.insert(env_var.as_ref(), "https://api.openai.com/v1".to_string());
            } else if env_var == &URLParam::from("ANTHROPIC_URL".to_owned()) {
                template_data.insert(env_var.as_ref(), "https://api.anthropic.com/v1".to_string());
            } else {
                return Err(ProviderError::env_var_not_found(config.id, env_var.as_ref()).into());
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
                forge_app::dto::Models::Url(model_url)
            }
            Models::Hardcoded(model_list) => forge_app::dto::Models::Hardcoded(model_list.clone()),
        };

        Ok(Provider {
            id: config.id,
            response: config.response_type.clone(),
            url: final_url,
            key: Some(api_key),
            models,
            auth_type: None,  // Environment-based providers don't track auth type
            credential: None, // Environment-based providers don't have database credentials
        })
    }

    async fn provider_from_id(&self, id: forge_app::dto::ProviderId) -> anyhow::Result<Provider> {
        // Handle special cases first
        if id == ProviderId::Forge {
            // Forge provider isn't typically configured via env vars in the registry
            return Err(ProviderError::provider_not_available(ProviderId::Forge).into());
        }

        // Try to create provider from database credential first
        if let Some(credential) = self.infra.get_credential(&id).await?
            && let Ok(provider) = self.create_provider_from_credential(&id, &credential).await {
                return Ok(provider);
            }

        // Database credential required - no environment variable fallback
        Err(ProviderError::provider_not_available(id).into())
    }

    async fn create_provider_from_credential(
        &self,
        provider_id: &ProviderId,
        credential: &forge_app::dto::ProviderCredential,
    ) -> anyhow::Result<Provider> {
        use forge_app::dto::AuthType;

        // Get provider config for URL templates
        let config = get_provider_configs()
            .iter()
            .find(|c| &c.id == provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider config not found for {:?}", provider_id))?;

        // Extract API key based on auth type
        let api_key = match credential.auth_type {
            AuthType::ApiKey => credential
                .api_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("API key missing for ApiKey auth type"))?,
            AuthType::OAuth => {
                // For OAuth, use access token as API key
                credential
                    .oauth_tokens
                    .as_ref()
                    .map(|tokens| tokens.access_token.as_str().to_string().into())
                    .ok_or_else(|| anyhow::anyhow!("OAuth tokens missing"))?
            }
            AuthType::OAuthWithApiKey => {
                // For OAuth+API Key (GitHub Copilot), use the stored API key
                credential.api_key.clone().ok_or_else(|| {
                    anyhow::anyhow!("API key missing for OAuthWithApiKey auth type")
                })?
            }
        };

        // Build template data from URL parameters
        let mut template_data = std::collections::HashMap::new();
        for (key, value) in &credential.url_params {
            template_data.insert(key.clone(), value.clone());
        }

        // Add default URLs if not present
        if !template_data.contains_key(&("OPENAI_URL".to_string().into())) {
            template_data.insert(
                "OPENAI_URL".to_owned().into(),
                "https://api.openai.com/v1".to_owned().into(),
            );
        }
        if !template_data.contains_key(&("ANTHROPIC_URL".to_string().into())) {
            template_data.insert(
                "ANTHROPIC_URL".to_owned().into(),
                "https://api.anthropic.com/v1".to_owned().into(),
            );
        }

        // Render URLs using handlebars
        let url = self
            .handlebars
            .render_template(&config.url, &template_data)
            .map_err(|e| {
                anyhow::anyhow!("Failed to render URL template for {:?}: {}", provider_id, e)
            })?;

        let models = match &config.models {
            Models::Url(url_template) => {
                let url = self
                    .handlebars
                    .render_template(url_template, &template_data)
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to render models URL template for {:?}: {}",
                            provider_id,
                            e
                        )
                    })?;
                forge_app::dto::Models::Url(Url::parse(&url)?)
            }
            Models::Hardcoded(models) => forge_app::dto::Models::Hardcoded(models.clone()),
        };

        Ok(Provider {
            id: *provider_id,
            response: config.response_type.clone(),
            url: Url::parse(&url)?,
            key: Some(api_key),
            models,
            auth_type: Some(credential.auth_type.clone()),
            credential: Some(credential.clone()),
        })
    }

    async fn get_first_available_provider(&self) -> anyhow::Result<Provider> {
        // Get all providers (database first, then env fallback)
        let all_providers = self.get_all_providers().await?;
        all_providers
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
impl<
    F: EnvironmentInfra
        + AppConfigRepository
        + FileReaderInfra
        + ProviderCredentialRepository
        + 'static,
> ProviderRegistry for ForgeProviderRegistry<F>
{
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
        use std::collections::HashMap;

        // Get all available provider IDs from configuration
        let available_provider_ids: Vec<ProviderId> = get_provider_configs()
            .iter()
            .filter(|config| config.id != ProviderId::Forge) // Exclude internal Forge provider
            .map(|config| config.id)
            .collect();

        // Get credentials
        let credential_map = self.infra.get_all_credentials().await?;

        // Get env-based providers
        let env_providers = self.get_providers().await.clone();
        let env_provider_map: HashMap<_, _> =
            env_providers.into_iter().map(|p| (p.id, p)).collect();

        let mut providers = Vec::new();

        for provider_id in available_provider_ids {
            // Priority: database credential > env-based provider > unconfigured provider
            if let Some(credential) = credential_map.get(&provider_id) {
                // Provider has database credential
                if let Ok(provider) = self
                    .create_provider_from_credential(&provider_id, credential)
                    .await
                {
                    providers.push(provider);
                }
            } else if let Some(env_provider) = env_provider_map.get(&provider_id) {
                // Provider configured via environment
                providers.push(env_provider.clone());
            } else {
                // Provider not configured - create a basic entry without credentials
                if let Some(config) = get_provider_configs().iter().find(|c| c.id == provider_id) {
                    // Create a minimal provider entry without credentials
                    let url = config.url.clone();
                    let models = match &config.models {
                        Models::Url(url_template) => {
                            if let Ok(parsed_url) = Url::parse(url_template) {
                                forge_app::dto::Models::Url(parsed_url)
                            } else {
                                continue; // Skip if URL is invalid
                            }
                        }
                        Models::Hardcoded(models) => {
                            forge_app::dto::Models::Hardcoded(models.clone())
                        }
                    };

                    if let Ok(parsed_url) = Url::parse(&url) {
                        providers.push(Provider {
                            id: provider_id,
                            response: config.response_type.clone(),
                            url: parsed_url,
                            key: None,
                            models,
                            auth_type: None,
                            credential: None,
                        });
                    }
                }
            }
        }

        Ok(providers)
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

    fn get_provider_auth_methods(
        &self,
        provider_id: &ProviderId,
    ) -> Vec<forge_app::dto::AuthMethod> {
        get_provider_auth_methods(provider_id)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_github_copilot_config() {
        let configs = get_provider_configs();
        let config = configs
            .iter()
            .find(|c| c.id == ProviderId::GithubCopilot)
            .expect("GithubCopilot config should exist");
        assert_eq!(config.id, ProviderId::GithubCopilot);
        assert_eq!(config.url, "https://api.githubcopilot.com/chat/completions");
        match &config.models {
            Models::Url(url) => assert_eq!(url, "https://api.githubcopilot.com/models"),
            _ => panic!("Expected models URL"),
        }
    }

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
        let expected: Vec<URLParam> = serde_json::from_str("[]").unwrap();
        assert_eq!(openrouter_config.url_param_vars, expected);
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
        let expected: Vec<URLParam> = serde_json::from_str("[]").unwrap();
        assert_eq!(config.url_param_vars, expected);
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
        let expected: Vec<URLParam> =
            serde_json::from_str(r#"["PROJECT_ID", "LOCATION"]"#).unwrap();
        assert_eq!(config.url_param_vars, expected);
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
        let expected: Vec<URLParam> = serde_json::from_str(
            r#"["AZURE_RESOURCE_NAME", "AZURE_DEPLOYMENT_NAME", "AZURE_API_VERSION"]"#,
        )
        .unwrap();
        assert_eq!(config.url_param_vars, expected);
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
        ) -> anyhow::Result<(String, forge_fs::FileInfo)> {
            Err(anyhow::anyhow!("File not found"))
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

    #[async_trait::async_trait]
    impl ProviderCredentialRepository for MockInfra {
        async fn upsert_credential(
            &self,
            _provider_id: forge_app::dto::ProviderId,
            _credential: forge_app::dto::ProviderCredential,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_credential(
            &self,
            _provider_id: &forge_app::dto::ProviderId,
        ) -> anyhow::Result<Option<forge_app::dto::ProviderCredential>> {
            Ok(None) // No database credentials in tests
        }

        async fn get_all_credentials(
            &self,
        ) -> anyhow::Result<
            std::collections::HashMap<
                forge_app::dto::ProviderId,
                forge_app::dto::ProviderCredential,
            >,
        > {
            Ok(std::collections::HashMap::new())
        }

        async fn update_oauth_tokens(
            &self,
            _provider_id: &forge_app::dto::ProviderId,
            _tokens: forge_app::dto::OAuthTokens,
        ) -> anyhow::Result<()> {
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

        // Get Azure config from embedded configs
        let configs = get_provider_configs();
        let azure_config = configs
            .iter()
            .find(|c| c.id == ProviderId::Azure)
            .expect("Azure config should exist");

        // Create provider using the registry's test_create_provider_from_env method
        let provider = registry
            .create_provider_from_env(azure_config)
            .expect("Should create Azure provider");

        // Verify all URLs are correctly rendered
        assert_eq!(provider.id, ProviderId::Azure);
        assert_eq!(provider.key, Some("test-key-123".to_string().into()));

        // Check chat completion URL (url field now contains the chat completion URL)
        let chat_url = provider.url;
        assert_eq!(
            chat_url.as_str(),
            "https://my-test-resource.openai.azure.com/openai/deployments/gpt-4-deployment/chat/completions?api-version=2024-02-01-preview"
        );

        // Check model URL
        match provider.models {
            forge_app::dto::Models::Url(model_url) => {
                assert_eq!(
                    model_url.as_str(),
                    "https://my-test-resource.openai.azure.com/openai/models?api-version=2024-02-01-preview"
                );
            }
            forge_app::dto::Models::Hardcoded(_) => panic!("Expected Models::Url variant"),
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
