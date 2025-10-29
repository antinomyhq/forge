use std::sync::{Arc, OnceLock};

use forge_app::ProviderRegistry;
use forge_app::domain::{AgentId, ModelId};
use forge_app::dto::{Provider, ProviderId, ProviderResponse, URLParam};
use handlebars::Handlebars;
use merge::Merge;
use serde::Deserialize;
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
        Self { infra, handlebars: get_handlebars() }
    }

    /// Loads provider configs from the base directory if they exist
    async fn get_custom_provider_configs(&self) -> anyhow::Result<Vec<ProviderConfig>> {
        let environment = self.infra.get_environment();
        let provider_json_path = environment.base_path.join("provider.json");

        let json_str = self.infra.read_utf8(&provider_json_path).await?;
        let configs = serde_json::from_str(&json_str)?;
        Ok(configs)
    }

    async fn provider_from_id(&self, id: forge_app::dto::ProviderId) -> anyhow::Result<Provider> {
        // Handle special cases first
        if id == ProviderId::Forge {
            // Forge provider isn't typically configured via env vars in the registry
            return Err(ProviderError::provider_not_available(ProviderId::Forge).into());
        }

        // Try to create provider from database credential first
        if let Some(credential) = self.infra.get_credential(&id).await?
            && let Ok(provider) = self.create_provider_from_credential(&id, &credential).await
        {
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
        let template_data = credential.url_params.clone();

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
            auth_methods: get_provider_auth_methods(provider_id),
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
        // Get all available provider configs (merged: embedded + custom)
        let all_configs = self.get_merged_configs().await;

        let available_provider_ids: Vec<ProviderId> = all_configs
            .iter()
            .filter(|config| config.id != ProviderId::Forge) // Exclude internal Forge provider
            .map(|config| config.id)
            .collect();

        // Get credentials from database
        let credential_map = self.infra.get_all_credentials().await?;

        let mut providers = Vec::new();

        for provider_id in available_provider_ids {
            if let Some(credential) = credential_map.get(&provider_id) {
                // Provider has database credential - use it
                if let Ok(provider) = self
                    .create_provider_from_credential(&provider_id, credential)
                    .await
                {
                    providers.push(provider);
                }
            } else {
                // Provider not configured - show ALL providers so users can configure them
                if let Some(config) = all_configs.iter().find(|c| c.id == provider_id) {
                    let empty_data = std::collections::HashMap::<String, String>::new();

                    // Try to render and parse the chat URL
                    // If it fails, use a template URL to show the original template
                    let parsed_url = self
                        .handlebars
                        .render_template(&config.url, &empty_data)
                        .ok()
                        .and_then(|url_str| Url::parse(&url_str).ok())
                        .unwrap_or_else(|| {
                            // Use template:// scheme to preserve the original template
                            Url::parse(&format!("template://{}", config.url.replace("://", "___")))
                                .unwrap()
                        });

                    // Try to render and parse the models URL
                    let models = match &config.models {
                        Models::Url(url_template) => {
                            let parsed_model_url = self
                                .handlebars
                                .render_template(url_template, &empty_data)
                                .ok()
                                .and_then(|rendered_url| Url::parse(&rendered_url).ok())
                                .unwrap_or_else(|| {
                                    // Use template:// scheme to preserve the original template
                                    Url::parse(&format!(
                                        "template://{}",
                                        url_template.replace("://", "___")
                                    ))
                                    .unwrap()
                                });
                            forge_app::dto::Models::Url(parsed_model_url)
                        }
                        Models::Hardcoded(models) => {
                            forge_app::dto::Models::Hardcoded(models.clone())
                        }
                    };

                    providers.push(Provider {
                        id: provider_id,
                        response: config.response_type.clone(),
                        url: parsed_url,
                        key: None,
                        models,
                        auth_methods: get_provider_auth_methods(&provider_id),
                        credential: None,
                    });
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
    fn test_create_azure_provider_with_credentials() {
        let env_vars = HashMap::new();
        let infra = Arc::new(MockInfra { env_vars });
        let registry = ForgeProviderRegistry::new(infra);

        // Create credential with Azure parameters
        let mut url_params = std::collections::HashMap::new();
        url_params.insert(
            "AZURE_RESOURCE_NAME".to_string().into(),
            "my-test-resource".to_string().into(),
        );
        url_params.insert(
            "AZURE_DEPLOYMENT_NAME".to_string().into(),
            "gpt-4-deployment".to_string().into(),
        );
        url_params.insert(
            "AZURE_API_VERSION".to_string().into(),
            "2024-02-01-preview".to_string().into(),
        );

        let credential =
            forge_app::dto::ProviderCredential::new_api_key("test-key-123".to_string())
                .url_params(url_params);

        // Create provider using credentials
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let provider = runtime
            .block_on(async {
                registry
                    .create_provider_from_credential(&ProviderId::Azure, &credential)
                    .await
            })
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
    async fn test_all_providers_shown_when_unconfigured() {
        // Test that ALL providers from provider.json are shown, even if unconfigured
        let env_vars = HashMap::new(); // Empty environment, no credentials
        let infra = Arc::new(MockInfra { env_vars });
        let registry = ForgeProviderRegistry::new(infra);
        let providers = registry.get_all_providers().await.unwrap();

        // ALL providers should appear (except Forge which is internal)
        let expected_providers = vec![
            ProviderId::GithubCopilot,
            ProviderId::OpenRouter,
            ProviderId::Requesty,
            ProviderId::Xai,
            ProviderId::OpenAI,
            ProviderId::OpenAICompatible, // Now shown!
            ProviderId::Anthropic,
            ProviderId::AnthropicCompatible, // Now shown!
            ProviderId::Cerebras,
            ProviderId::Zai,
            ProviderId::ZaiCoding,
            ProviderId::BigModel,
            ProviderId::VertexAi, // Now shown!
            ProviderId::Azure,    // Now shown!
        ];

        let actual_provider_ids: Vec<ProviderId> = providers.iter().map(|p| p.id).collect();

        // Check that ALL providers appear
        for expected_id in &expected_providers {
            assert!(
                actual_provider_ids.contains(expected_id),
                "Provider {:?} should be present (from provider.json)",
                expected_id
            );
        }

        // Verify unconfigured providers have no credentials
        for provider in &providers {
            assert!(
                provider.credential.is_none(),
                "Unconfigured provider {:?} should not have credentials",
                provider.id
            );
        }
    }
}
