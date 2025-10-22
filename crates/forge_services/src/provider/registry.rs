use std::sync::{Arc, OnceLock};

use forge_app::ProviderRegistry;
use forge_app::domain::{AgentId, ModelId};
use forge_app::dto::{Provider, ProviderId, ProviderResponse, URLParam};
use handlebars::Handlebars;
use serde::Deserialize;
use tokio::sync::OnceCell;
use tracing;
use url::Url;

use crate::{AppConfigRepository, EnvironmentInfra, ProviderCredentialRepository, ProviderError};

#[derive(Debug, Deserialize)]
pub struct ProviderConfig {
    pub(crate) id: ProviderId,
    pub(crate) display_name: String,
    pub(crate) api_key_vars: String,
    pub(crate) url_param_vars: Vec<URLParam>,
    pub(crate) response_type: ProviderResponse,
    pub(crate) url: String,
    pub(crate) model_url: String,
    pub(crate) auth_methods: Vec<crate::provider::AuthMethod>,
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
            .map_err(|e| anyhow::anyhow!("Failed to parse provider configs: {}", e))
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
        .map(|config| (config.api_key_vars.clone(), config.url_param_vars.clone()))
}

/// Get display name for a provider
pub fn get_provider_display_name(provider_id: &ProviderId) -> String {
    get_provider_config(provider_id)
        .map(|config| config.display_name.clone())
        .unwrap_or_else(|| format!("{:?}", provider_id))
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

impl<F: EnvironmentInfra + AppConfigRepository + ProviderCredentialRepository + 'static>
    ForgeProviderRegistry<F>
{
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
                // Note: This is synchronous initialization, only loads env-based providers
                // For database credentials, use provider_from_id() which is async
                self.create_provider_from_env(config).ok()
            })
            .collect()
    }

    fn create_provider_from_env(&self, config: &ProviderConfig) -> anyhow::Result<Provider> {
        // Check API key environment variable
        let api_key = self
            .infra
            .get_env_var(&config.api_key_vars)
            .ok_or_else(|| {
                ProviderError::env_var_not_found(config.id.clone(), &config.api_key_vars)
            })?;

        // Check URL parameter environment variables and build template data
        // URL parameters are optional - only add them if they exist
        let mut template_data = std::collections::HashMap::new();

        for env_var in &config.url_param_vars {
            if let Some(value) = self.infra.get_env_var(env_var.as_ref()) {
                template_data.insert(env_var.as_ref(), value);
            } else if **env_var == "OPENAI_URL" {
                template_data.insert(env_var.as_ref(), "https://api.openai.com/v1".to_string());
            } else if **env_var == "ANTHROPIC_URL" {
                template_data.insert(env_var.as_ref(), "https://api.anthropic.com/v1".to_string());
            } else {
                return Err(
                    ProviderError::env_var_not_found(config.id.clone(), env_var.as_ref()).into(),
                );
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
            id: config.id.clone(),
            response: config.response_type.clone(),
            url: final_url,
            key: Some(api_key),
            model_url,
            auth_type: None, // Environment-based providers don't track auth type
        })
    }

    async fn provider_from_id(&self, id: forge_app::dto::ProviderId) -> anyhow::Result<Provider> {
        // Handle special cases first
        if id == ProviderId::Forge {
            // Forge provider isn't typically configured via env vars in the registry
            return Err(ProviderError::provider_not_available(ProviderId::Forge).into());
        }

        // Try to create provider from database credential first
        if let Some(mut credential) = self.infra.get_credential(&id).await? {
            // Check if OAuth tokens need refresh (within 5 minutes of expiry)
            if credential.needs_token_refresh() {
                tracing::debug!(provider = ?id, "OAuth token needs refresh, attempting to refresh");

                // Attempt to refresh tokens
                match self.refresh_credential_tokens(&credential).await {
                    Ok(refreshed_credential) => {
                        tracing::info!(provider = ?id, "Successfully refreshed OAuth tokens");
                        credential = refreshed_credential;
                    }
                    Err(e) => {
                        // Log error but don't fail - the existing token might still work
                        tracing::warn!(
                            provider = ?id,
                            error = %e,
                            "Failed to refresh OAuth tokens, will attempt with existing credential"
                        );
                    }
                }
            }

            if let Ok(provider) = self.create_provider_from_credential(&id, &credential).await {
                return Ok(provider);
            }
        }

        // Database credential required - no environment variable fallback
        Err(ProviderError::provider_not_available(id).into())
    }

    /// Refreshes OAuth tokens for a credential
    ///
    /// Handles both standard OAuth refresh and GitHub Copilot API key refresh
    ///
    /// # Arguments
    ///
    /// * `provider_id` - The provider ID
    /// * `credential` - The credential with tokens to refresh
    ///
    /// # Returns
    ///
    /// Updated credential with refreshed tokens
    ///
    /// # Errors
    ///
    /// Returns error if refresh fails or provider metadata not found
    async fn refresh_credential_tokens(
        &self,
        credential: &forge_app::dto::ProviderCredential,
    ) -> anyhow::Result<forge_app::dto::ProviderCredential> {
        tracing::debug!(provider = ?credential.provider_id, "Refreshing credential tokens");

        // Get authentication method from provider config
        let auth_methods = get_provider_auth_methods(&credential.provider_id);
        let auth_method = auth_methods.first().ok_or_else(|| {
            anyhow::anyhow!(
                "No authentication method found for provider {:?}",
                credential.provider_id
            )
        })?;

        // Create provider auth service
        let auth_service = crate::provider::ForgeProviderAuthService::new(self.infra.clone());

        // Use service to refresh the credential (call trait method explicitly)
        use forge_app::ProviderAuthService as _;
        let refreshed_credential = auth_service
            .refresh_provider_credential(credential, auth_method.clone())
            .await?;

        Ok(refreshed_credential)
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
                    .map(|tokens| tokens.access_token.clone())
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
            let upper_case = key.as_str().to_uppercase();
            template_data.insert(upper_case, value.clone());
        }

        // Add default URLs if not present
        if !template_data.contains_key("OPENAI_URL") {
            template_data.insert("OPENAI_URL".into(), "https://api.openai.com/v1".to_string());
        }
        if !template_data.contains_key("ANTHROPIC_URL") {
            template_data.insert(
                "ANTHROPIC_URL".into(),
                "https://api.anthropic.com/v1".to_string(),
            );
        }

        // Render URLs using handlebars
        let url = self
            .handlebars
            .render_template(&config.url, &template_data)
            .map_err(|e| {
                anyhow::anyhow!("Failed to render URL template for {:?}: {}", provider_id, e)
            })?;

        let model_url = self
            .handlebars
            .render_template(&config.model_url, &template_data)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to render model_url template for {:?}: {}",
                    provider_id,
                    e
                )
            })?;

        Ok(Provider {
            id: provider_id.clone(),
            response: config.response_type.clone(),
            url: Url::parse(&url)?,
            key: Some(api_key),
            model_url: Url::parse(&model_url)?,
            auth_type: Some(credential.auth_type.clone()),
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
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + AppConfigRepository + ProviderCredentialRepository + 'static>
    ProviderRegistry for ForgeProviderRegistry<F>
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
        use std::collections::HashSet;

        // Start with database-based providers (highest priority)
        let db_credentials = self.infra.get_all_credentials().await?;
        let mut providers = Vec::new();
        let mut provider_ids: HashSet<ProviderId> = HashSet::new();

        for credential in db_credentials {
            // Try to create provider from credential
            if let Ok(provider) = self
                .create_provider_from_credential(&credential.provider_id, &credential)
                .await
            {
                providers.push(provider);
                provider_ids.insert(credential.provider_id.clone());
            }
        }

        // Add env-based providers that aren't already in the list (fallback)
        let env_providers = self.get_providers().await.clone();
        for provider in env_providers {
            if !provider_ids.contains(&provider.id) {
                provider_ids.insert(provider.id.clone());
                providers.push(provider);
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

    async fn available_provider_ids(&self) -> Vec<ProviderId> {
        // Get built-in providers
        let provider_ids: Vec<ProviderId> = get_provider_configs()
            .iter()
            .filter(|config| config.id != ProviderId::Forge) // Exclude internal Forge provider
            .map(|config| config.id.clone())
            .collect();

        // Note: Custom URLs don't add new provider IDs - they just override existing
        // ones So we don't need to add anything here

        provider_ids
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
        assert_eq!(config.model_url, "https://api.githubcopilot.com/models");
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

    #[async_trait::async_trait]
    impl ProviderCredentialRepository for MockInfra {
        async fn upsert_credential(
            &self,
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
        ) -> anyhow::Result<Vec<forge_app::dto::ProviderCredential>> {
            Ok(Vec::new())
        }

        async fn mark_verified(
            &self,
            _provider_id: &forge_app::dto::ProviderId,
        ) -> anyhow::Result<()> {
            Ok(())
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

        // Get Azure config
        let configs = get_provider_configs();
        let azure_config = configs
            .iter()
            .find(|c| c.id == ProviderId::Azure)
            .expect("Azure config should exist");

        // Create provider using the registry's create_provider_from_env method
        let provider = registry
            .create_provider_from_env(azure_config)
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
