use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

use forge_app::ProviderRegistry;
use forge_app::domain::{AgentId, ModelId};
use forge_app::dto::{Provider, ProviderId, ProviderResponse};
use handlebars::Handlebars;
use serde::Deserialize;
use tokio::sync::OnceCell;
use tracing;
use url::Url;

use crate::provider::{AuthFlowFactory, ForgeOAuthService, GitHubCopilotService};
use crate::{
    AppConfigRepository, EnvironmentInfra, ProviderCredentialRepository, ProviderError,
    ProviderSpecificProcessingInfra,
};

#[derive(Debug, Deserialize)]
pub(crate) struct ProviderConfig {
    pub(crate) id: ProviderId,
    pub(crate) api_key_vars: String,
    pub(crate) url_param_vars: Vec<String>,
    pub(crate) response_type: ProviderResponse,
    pub(crate) url: String,
    pub(crate) model_url: String,
}

static HANDLEBARS: OnceLock<Handlebars<'static>> = OnceLock::new();
static PROVIDER_CONFIGS: OnceLock<Vec<ProviderConfig>> = OnceLock::new();
static ENV_VAR_WARNINGS: OnceLock<Mutex<HashSet<ProviderId>>> = OnceLock::new();

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

fn get_env_var_warnings() -> &'static Mutex<HashSet<ProviderId>> {
    ENV_VAR_WARNINGS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn log_env_var_deprecation_warning(provider_id: ProviderId) {
    let mut warned = get_env_var_warnings().lock().unwrap();

    // Only warn once per provider per session
    if warned.insert(provider_id.clone()) {
        eprintln!(
            "⚠️  Warning: Using environment variable for {}. \
            Run `forge auth import-env` to migrate to secure storage.",
            provider_id
        );
    }
}

pub struct ForgeProviderRegistry<F> {
    infra: Arc<F>,
    handlebars: &'static Handlebars<'static>,
    providers: OnceCell<Vec<Provider>>,
}

/// Infrastructure adapter for auth flows within the registry.
///
/// This adapter provides the required services (OAuth, GitHub Copilot)
/// needed by authentication flows for token refresh operations.
struct RegistryInfraAdapter {
    oauth_service: Arc<ForgeOAuthService>,
    github_service: Arc<GitHubCopilotService>,
}

impl crate::provider::auth_flow::AuthFlowInfra for RegistryInfraAdapter {
    fn oauth_service(&self) -> Arc<ForgeOAuthService> {
        self.oauth_service.clone()
    }

    fn github_copilot_service(&self) -> Arc<GitHubCopilotService> {
        self.github_service.clone()
    }
}

impl<
    F: EnvironmentInfra
        + AppConfigRepository
        + ProviderCredentialRepository
        + ProviderSpecificProcessingInfra,
> ForgeProviderRegistry<F>
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
            if let Some(value) = self.infra.get_env_var(env_var) {
                template_data.insert(env_var, value);
            } else if env_var == "OPENAI_URL" {
                template_data.insert(env_var, "https://api.openai.com/v1".to_string());
            } else if env_var == "ANTHROPIC_URL" {
                template_data.insert(env_var, "https://api.anthropic.com/v1".to_string());
            } else {
                return Err(ProviderError::env_var_not_found(config.id.clone(), env_var).into());
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
                match self.refresh_credential_tokens(&id, &credential).await {
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

        // Fall back to cached env-based providers
        let providers = self.get_providers().await;
        match providers.iter().find(|p| p.id == id).cloned() {
            Some(provider) => {
                // Log deprecation warning for env-var based providers
                log_env_var_deprecation_warning(id);
                Ok(provider)
            }
            None => Err(ProviderError::provider_not_available(id).into()),
        }
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
        provider_id: &ProviderId,
        credential: &forge_app::dto::ProviderCredential,
    ) -> anyhow::Result<forge_app::dto::ProviderCredential> {
        tracing::debug!(provider = ?provider_id, "Refreshing credential tokens");

        // Get authentication method from metadata
        let metadata = self.infra.get_provider_metadata(provider_id);
        let auth_method = metadata.auth_methods.first().ok_or_else(|| {
            anyhow::anyhow!(
                "No authentication method found for provider {:?}",
                provider_id
            )
        })?;

        // Create an infrastructure adapter for the auth flow
        let infra_adapter = RegistryInfraAdapter {
            oauth_service: Arc::new(ForgeOAuthService),
            github_service: Arc::new(GitHubCopilotService::new()),
        };

        // Create the appropriate auth flow using the factory
        let flow = AuthFlowFactory::create_flow(provider_id, auth_method, Arc::new(infra_adapter))?;

        // Use the flow's refresh method
        let refreshed_credential = flow.refresh(credential).await?;

        // Update credential in database
        self.infra
            .upsert_credential(refreshed_credential.clone())
            .await?;

        Ok(refreshed_credential)
    }

    async fn create_provider_from_credential(
        &self,
        provider_id: &ProviderId,
        credential: &forge_app::dto::ProviderCredential,
    ) -> anyhow::Result<Provider> {
        use forge_app::dto::AuthType;

        // Handle custom providers separately
        if provider_id.is_custom() {
            return self.create_custom_provider(provider_id, credential);
        }

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
            template_data.insert(key.as_str(), value.clone());
        }

        // Add default URLs if not present
        if !template_data.contains_key("OPENAI_URL") {
            template_data.insert("OPENAI_URL", "https://api.openai.com/v1".to_string());
        }
        if !template_data.contains_key("ANTHROPIC_URL") {
            template_data.insert("ANTHROPIC_URL", "https://api.anthropic.com/v1".to_string());
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
        })
    }

    /// Creates a Provider instance for custom user-defined providers
    ///
    /// # Arguments
    /// * `provider_id` - Custom provider ID
    /// * `credential` - Provider credential with custom provider metadata
    ///
    /// # Returns
    /// A Provider configured to use the custom base URL and model ID
    ///
    /// # Errors
    /// Returns error if required custom provider fields are missing
    fn create_custom_provider(
        &self,
        provider_id: &ProviderId,
        credential: &forge_app::dto::ProviderCredential,
    ) -> anyhow::Result<Provider> {
        use forge_app::dto::CompatibilityMode;

        // Validate custom provider has required fields
        let base_url = credential.custom_base_url.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Missing base_url for custom provider {:?}", provider_id)
        })?;

        let _model_id = credential.custom_model_id.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Missing model_id for custom provider {:?}", provider_id)
        })?;

        let compatibility_mode = credential.compatibility_mode.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Missing compatibility_mode for custom provider {:?}",
                provider_id
            )
        })?;

        // Determine response type based on compatibility mode
        let response_type = match compatibility_mode {
            CompatibilityMode::OpenAI => ProviderResponse::OpenAI,
            CompatibilityMode::Anthropic => ProviderResponse::Anthropic,
        };

        // Build chat completions and models URLs based on compatibility mode
        let (chat_url, models_url) = match compatibility_mode {
            CompatibilityMode::OpenAI => {
                // OpenAI-compatible: /v1/chat/completions and /v1/models
                let chat = format!("{}/chat/completions", base_url.trim_end_matches('/'));
                let models = format!("{}/models", base_url.trim_end_matches('/'));
                (chat, models)
            }
            CompatibilityMode::Anthropic => {
                // Anthropic-compatible: /v1/messages and /v1/models
                let chat = format!("{}/messages", base_url.trim_end_matches('/'));
                let models = format!("{}/models", base_url.trim_end_matches('/'));
                (chat, models)
            }
        };

        // API key is optional for custom providers (local servers may not require auth)
        let api_key = credential.api_key.clone();

        Ok(Provider {
            id: provider_id.clone(),
            response: response_type,
            url: Url::parse(&chat_url).map_err(|e| {
                anyhow::anyhow!("Invalid custom provider URL '{}': {}", chat_url, e)
            })?,
            key: api_key,
            model_url: Url::parse(&models_url).map_err(|e| {
                anyhow::anyhow!("Invalid custom provider model URL '{}': {}", models_url, e)
            })?,
        })
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

    /// Lists all custom provider credentials
    ///
    /// # Returns
    /// Vector of credentials for custom providers only (ProviderId::Custom)
    ///
    /// # Errors
    /// Returns error if database operation fails
    pub async fn list_custom_providers(
        &self,
    ) -> anyhow::Result<Vec<forge_app::dto::ProviderCredential>> {
        let all_credentials = self.infra.get_all_credentials().await?;
        Ok(all_credentials
            .into_iter()
            .filter(|c| c.is_custom_provider())
            .collect())
    }

    /// Deletes a custom provider credential
    ///
    /// # Arguments
    /// * `provider_id` - The custom provider ID to delete
    ///
    /// # Returns
    /// Ok if deleted successfully
    ///
    /// # Errors
    /// * Returns error if provider_id is not a custom provider
    /// * Returns error if database operation fails
    pub async fn delete_custom_provider(
        &self,
        provider_id: &forge_app::dto::ProviderId,
    ) -> anyhow::Result<()> {
        if !provider_id.is_custom() {
            return Err(anyhow::anyhow!(
                "Cannot delete built-in provider: {}",
                provider_id
            ));
        }

        self.infra.delete_credential(provider_id).await?;

        // If this was the active provider, clear it
        let app_config = self.infra.get_app_config().await?;
        if app_config.provider.as_ref() == Some(provider_id) {
            self.update(|config| {
                config.provider = None;
            })
            .await?;
        }

        Ok(())
    }

    /// Stores a custom provider credential
    ///
    /// # Arguments
    /// * `credential` - The custom provider credential to store
    ///
    /// # Returns
    /// Ok if stored successfully
    ///
    /// # Errors
    /// * Returns error if credential is not for a custom provider
    /// * Returns error if database operation fails
    pub async fn store_custom_provider(
        &self,
        credential: forge_app::dto::ProviderCredential,
    ) -> anyhow::Result<()> {
        if !credential.is_custom_provider() {
            return Err(anyhow::anyhow!(
                "Cannot store as custom provider: credential is for built-in provider {}",
                credential.provider_id
            ));
        }

        self.infra.upsert_credential(credential).await
    }
}

#[async_trait::async_trait]
impl<
    F: EnvironmentInfra
        + AppConfigRepository
        + ProviderCredentialRepository
        + ProviderSpecificProcessingInfra,
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
        use std::collections::HashSet;

        // Start with env-based providers
        let mut providers = self.get_providers().await.clone();
        let mut provider_ids: HashSet<ProviderId> =
            providers.iter().map(|p| p.id.clone()).collect();

        // Add database-based providers that aren't already in the list
        let db_credentials = self.infra.get_all_credentials().await?;
        for credential in db_credentials {
            if !provider_ids.contains(&credential.provider_id) {
                // Try to create provider from credential
                if let Ok(provider) = self
                    .create_provider_from_credential(&credential.provider_id, &credential)
                    .await
                {
                    providers.push(provider);
                    provider_ids.insert(credential.provider_id);
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

    async fn available_provider_ids(&self) -> Vec<ProviderId> {
        // Get built-in providers
        let mut provider_ids: Vec<ProviderId> = get_provider_configs()
            .iter()
            .filter(|config| config.id != ProviderId::Forge) // Exclude internal Forge provider
            .map(|config| config.id.clone())
            .collect();

        // Add custom providers from credentials (they're already registered)
        if let Ok(credentials) = self.infra.get_all_credentials().await {
            for cred in credentials {
                if cred.provider_id.is_custom() && !provider_ids.contains(&cred.provider_id) {
                    provider_ids.push(cred.provider_id.clone());
                }
            }
        }

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

    use anyhow::bail;
    use chrono::{DateTime, Utc};
    use forge_app::domain::Environment;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::infra::ProviderSpecificProcessingInfra;
    use crate::provider::{ProviderMetadata, ProviderMetadataService};

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
    impl ProviderSpecificProcessingInfra for MockInfra {
        async fn process_github_copilot_token(
            &self,
            _access_token: &str,
        ) -> anyhow::Result<(String, Option<DateTime<Utc>>)> {
            bail!("GitHub Copilot processing not supported in MockInfra")
        }

        fn get_provider_metadata(&self, provider_id: &ProviderId) -> ProviderMetadata {
            ProviderMetadataService::get_metadata(provider_id)
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

        async fn delete_credential(
            &self,
            _provider_id: &forge_app::dto::ProviderId,
        ) -> anyhow::Result<()> {
            Ok(())
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

    #[tokio::test]
    async fn test_deprecation_warning_for_env_vars() {
        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

        let infra = Arc::new(MockInfra { env_vars });
        let registry = ForgeProviderRegistry::new(infra);

        // First call should trigger warning (we can't easily capture stderr in test,
        // but we can verify the provider is created successfully)
        let provider1 = registry.provider_from_id(ProviderId::OpenAI).await.unwrap();
        assert_eq!(provider1.id, ProviderId::OpenAI);

        // Second call in same session - should still work (warning suppressed
        // internally)
        let provider2 = registry.provider_from_id(ProviderId::OpenAI).await.unwrap();
        assert_eq!(provider2.id, ProviderId::OpenAI);

        // Verify warning tracking state exists (warning was logged at least once)
        let warned = get_env_var_warnings().lock().unwrap();
        assert!(warned.contains(&ProviderId::OpenAI));
    }

    #[tokio::test]
    async fn test_create_custom_provider_openai_compatible() {
        use forge_app::dto::{CompatibilityMode, ProviderCredential};

        let infra = Arc::new(MockInfra { env_vars: HashMap::new() });
        let registry = ForgeProviderRegistry::new(infra);

        let credential = ProviderCredential::new_custom_provider(
            ProviderId::Custom("LocalAI".to_string()),
            Some("test-api-key".to_string()),
            CompatibilityMode::OpenAI,
            "http://localhost:8080/v1".to_string(),
            "gpt-4-local".to_string(),
        );

        let provider = registry
            .create_custom_provider(&credential.provider_id, &credential)
            .unwrap();

        assert_eq!(provider.id, ProviderId::Custom("LocalAI".to_string()));
        assert_eq!(provider.response, ProviderResponse::OpenAI);
        assert_eq!(
            provider.url.as_str(),
            "http://localhost:8080/v1/chat/completions"
        );
        assert_eq!(
            provider.model_url.as_str(),
            "http://localhost:8080/v1/models"
        );
        assert_eq!(provider.key, Some("test-api-key".to_string()));
    }

    #[tokio::test]
    async fn test_create_custom_provider_anthropic_compatible() {
        use forge_app::dto::{CompatibilityMode, ProviderCredential};

        let infra = Arc::new(MockInfra { env_vars: HashMap::new() });
        let registry = ForgeProviderRegistry::new(infra);

        let credential = ProviderCredential::new_custom_provider(
            ProviderId::Custom("Corporate Claude".to_string()),
            Some("corp-key".to_string()),
            CompatibilityMode::Anthropic,
            "https://llm.corp.example.com/api".to_string(),
            "claude-3-opus-internal".to_string(),
        );

        let provider = registry
            .create_custom_provider(&credential.provider_id, &credential)
            .unwrap();

        assert_eq!(
            provider.id,
            ProviderId::Custom("Corporate Claude".to_string())
        );
        assert_eq!(provider.response, ProviderResponse::Anthropic);
        assert_eq!(
            provider.url.as_str(),
            "https://llm.corp.example.com/api/messages"
        );
        assert_eq!(
            provider.model_url.as_str(),
            "https://llm.corp.example.com/api/models"
        );
        assert_eq!(provider.key, Some("corp-key".to_string()));
    }

    #[tokio::test]
    async fn test_create_custom_provider_without_api_key() {
        use forge_app::dto::{CompatibilityMode, ProviderCredential};

        let infra = Arc::new(MockInfra { env_vars: HashMap::new() });
        let registry = ForgeProviderRegistry::new(infra);

        let credential = ProviderCredential::new_custom_provider(
            ProviderId::Custom("Local Server".to_string()),
            None, // No API key for local server
            CompatibilityMode::OpenAI,
            "http://localhost:11434/v1".to_string(),
            "llama3".to_string(),
        );

        let provider = registry
            .create_custom_provider(&credential.provider_id, &credential)
            .unwrap();

        assert_eq!(provider.id, ProviderId::Custom("Local Server".to_string()));
        assert_eq!(provider.response, ProviderResponse::OpenAI);
        assert_eq!(provider.key, None); // No API key
    }

    #[tokio::test]
    async fn test_create_custom_provider_with_trailing_slash() {
        use forge_app::dto::{CompatibilityMode, ProviderCredential};

        let infra = Arc::new(MockInfra { env_vars: HashMap::new() });
        let registry = ForgeProviderRegistry::new(infra);

        let credential = ProviderCredential::new_custom_provider(
            ProviderId::Custom("Test".to_string()),
            None,
            CompatibilityMode::OpenAI,
            "http://localhost:8080/v1/".to_string(), // Trailing slash
            "model".to_string(),
        );

        let provider = registry
            .create_custom_provider(&credential.provider_id, &credential)
            .unwrap();

        // Should handle trailing slash correctly
        assert_eq!(
            provider.url.as_str(),
            "http://localhost:8080/v1/chat/completions"
        );
    }
}
