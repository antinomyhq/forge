/// DTOs for provider authentication
use forge_app::dto::{OAuthTokens, ProviderCredential, ProviderId};

/// Information to display to the user during OAuth device flow
#[derive(Debug, Clone)]
pub struct OAuthDeviceDisplay {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    pub success: bool,
    pub message: Option<String>,
}

impl ValidationOutcome {
    pub fn success() -> Self {
        Self { success: true, message: None }
    }

    pub fn success_with_message(message: impl Into<String>) -> Self {
        Self { success: true, message: Some(message.into()) }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self { success: false, message: Some(message.into()) }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImportSummary {
    pub imported: Vec<ProviderId>,
    pub failed: Vec<(ProviderId, String)>,
    pub skipped: Vec<ProviderId>,
}

impl ImportSummary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total_processed(&self) -> usize {
        self.imported.len() + self.failed.len() + self.skipped.len()
    }

    pub fn has_imports(&self) -> bool {
        !self.imported.is_empty()
    }

    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }
}

/// Provider authentication coordinator
///
/// This module provides a high-level authentication coordinator for providers
/// that uses centralized metadata for OAuth configuration and validation.
use std::sync::Arc;

use forge_app::ProviderRegistry;

use super::{
    ForgeOAuthService, ForgeProviderValidationService, GitHubCopilotService,
    ProviderMetadataService, ValidationResult,
};
use crate::infra::ProviderCredentialRepository;

/// Provider authenticator - coordinates provider authentication flows
///
/// This struct handles all provider authentication operations including:
/// - Adding API key credentials with validation
/// - OAuth device flow with callback-based display
/// - Importing credentials from environment variables
///
/// It uses `ProviderMetadataService` to get OAuth configurations and
/// environment variable names, ensuring no hardcoded configuration in the
/// authentication logic.
pub struct ProviderAuthenticator<S, I> {
    services: Arc<S>,
    infra: Arc<I>,
}

impl<S, I> ProviderAuthenticator<S, I>
where
    S: ProviderRegistry + Send + Sync,
    I: ProviderCredentialRepository + crate::infra::HttpInfra + Send + Sync,
{
    /// Creates a new provider authenticator
    ///
    /// # Arguments
    ///
    /// * `services` - Services providing provider registry
    /// * `infra` - Infrastructure providing credential storage and HTTP client
    pub fn new(services: Arc<S>, infra: Arc<I>) -> Self {
        Self { services, infra }
    }

    /// Add an API key credential with optional validation
    ///
    /// # Arguments
    ///
    /// * `provider_id` - Provider to add credential for
    /// * `api_key` - API key to add
    /// * `skip_validation` - If true, skip validation and add directly
    ///
    /// # Returns
    ///
    /// ValidationOutcome indicating success or failure
    ///
    /// # Errors
    ///
    /// Returns error if validation or credential storage fails
    pub async fn add_api_key_credential(
        &self,
        provider_id: ProviderId,
        api_key: String,
        skip_validation: bool,
    ) -> anyhow::Result<ValidationOutcome> {
        // Create credential
        let credential = ProviderCredential::new_api_key(provider_id, api_key);

        // Validate if requested
        if !skip_validation {
            // Get provider for validation URL
            let providers = self.services.get_all_providers().await?;
            let provider = providers
                .iter()
                .find(|p| p.id == provider_id)
                .ok_or_else(|| anyhow::anyhow!("Provider {} not found", provider_id))?;

            // Validate credential
            let validation_service = ForgeProviderValidationService::new(Arc::clone(&self.infra));
            let result = validation_service
                .validate_credential(&provider_id, &credential, &provider.model_url)
                .await?;

            match result {
                ValidationResult::Valid => {
                    // Save and return success
                    self.infra.upsert_credential(credential).await?;
                    Ok(ValidationOutcome::success_with_message(
                        "API key validated and saved",
                    ))
                }
                ValidationResult::Invalid(msg) => Ok(ValidationOutcome::failure(format!(
                    "API key validation failed: {}",
                    msg
                ))),
                ValidationResult::Inconclusive(msg) => {
                    // Save anyway but warn user
                    self.infra.upsert_credential(credential).await?;
                    Ok(ValidationOutcome::success_with_message(format!(
                        "API key saved (validation inconclusive: {})",
                        msg
                    )))
                }
                ValidationResult::TokenExpired => {
                    Ok(ValidationOutcome::failure("Token has expired"))
                }
            }
        } else {
            // Skip validation, save directly
            self.infra.upsert_credential(credential).await?;
            Ok(ValidationOutcome::success_with_message(
                "API key saved without validation",
            ))
        }
    }

    /// Authenticate with OAuth device flow (single-method design)
    ///
    /// This method handles the complete OAuth device flow:
    /// 1. Initiates device authorization
    /// 2. Calls display_callback with user_code and verification_uri
    /// 3. Polls for authorization completion (oauth2 crate handles loop)
    /// 4. Performs provider-specific post-processing (e.g., GitHub Copilot)
    /// 5. Saves the credential
    ///
    /// The oauth2 crate handles all polling logic internally with exponential
    /// backoff. This method will BLOCK until the user authorizes or it times
    /// out.
    ///
    /// # Arguments
    ///
    /// * `provider_id` - Provider to authenticate with
    /// * `display_callback` - Callback to display user_code and
    ///   verification_uri to user
    ///
    /// # Returns
    ///
    /// Success when credential is saved
    ///
    /// # Errors
    ///
    /// Returns error if OAuth flow fails, authorization denied, or credential
    /// save fails
    pub async fn authenticate_with_oauth<F>(
        &self,
        provider_id: ProviderId,
        display_callback: F,
    ) -> anyhow::Result<()>
    where
        F: FnOnce(OAuthDeviceDisplay),
    {
        // Get OAuth config from metadata service
        let auth_method = ProviderMetadataService::get_oauth_method(&provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider {} does not support OAuth", provider_id))?;

        let oauth_config = auth_method
            .oauth_config
            .ok_or_else(|| anyhow::anyhow!("OAuth config missing"))?;

        // Call OAuth service - this handles everything in one shot
        let oauth_service = ForgeOAuthService::new();
        let oauth_tokens = oauth_service
            .device_flow_with_callback(&oauth_config, display_callback)
            .await?;

        // Provider-specific post-processing
        let credential = self
            .process_provider_specific_oauth(provider_id, oauth_tokens)
            .await?;

        // Save credential
        self.infra.upsert_credential(credential).await?;

        Ok(())
    }

    /// Handles provider-specific OAuth post-processing
    ///
    /// Some providers (like GitHub Copilot) require additional steps after
    /// OAuth token exchange.
    ///
    /// # Arguments
    ///
    /// * `provider_id` - Provider ID
    /// * `oauth_tokens` - OAuth tokens from device flow
    ///
    /// # Returns
    ///
    /// Provider credential ready to save
    ///
    /// # Errors
    ///
    /// Returns error if provider-specific processing fails
    async fn process_provider_specific_oauth(
        &self,
        provider_id: ProviderId,
        oauth_tokens: OAuthTokens,
    ) -> anyhow::Result<ProviderCredential> {
        match provider_id {
            ProviderId::GithubCopilot => {
                // GitHub Copilot requires exchanging OAuth token for API key
                let copilot_service = GitHubCopilotService::new();
                let (api_key, expires_at) = copilot_service
                    .get_copilot_api_key(&oauth_tokens.access_token)
                    .await?;

                // Store the GitHub OAuth token as refresh_token for later use
                // The API key gets stored separately
                let copilot_tokens = OAuthTokens {
                    access_token: oauth_tokens.access_token.clone(), // GitHub OAuth token
                    refresh_token: oauth_tokens.refresh_token,
                    expires_at, // Use Copilot API key expiry
                };

                // Use new_oauth_with_api_key to set AuthType::OAuthWithApiKey
                Ok(ProviderCredential::new_oauth_with_api_key(
                    provider_id,
                    api_key,
                    copilot_tokens,
                ))
            }
            _ => {
                // Standard OAuth flow - save tokens directly
                Ok(ProviderCredential::new_oauth(provider_id, oauth_tokens))
            }
        }
    }

    /// Import provider credentials from environment variables
    ///
    /// Uses provider metadata to determine which environment variables to
    /// check for each provider.
    ///
    /// # Arguments
    ///
    /// * `filter` - Optional provider ID to import only specific provider
    ///
    /// # Returns
    ///
    /// Summary of imported, failed, and skipped providers
    ///
    /// # Errors
    ///
    /// Returns error if credential storage fails
    pub async fn import_from_env(
        &self,
        filter: Option<ProviderId>,
    ) -> anyhow::Result<ImportSummary> {
        let mut summary = ImportSummary::new();

        // Get all provider IDs
        let all_provider_ids = vec![
            ProviderId::Forge,
            ProviderId::GithubCopilot,
            ProviderId::OpenAI,
            ProviderId::Anthropic,
            ProviderId::OpenRouter,
            ProviderId::Requesty,
            ProviderId::Zai,
            ProviderId::ZaiCoding,
            ProviderId::Cerebras,
            ProviderId::Xai,
            ProviderId::VertexAi,
            ProviderId::BigModel,
            ProviderId::Azure,
        ];

        for provider_id in all_provider_ids {
            // Apply filter if provided
            if let Some(filter_id) = filter
                && filter_id != provider_id
            {
                continue;
            }

            // Get env var names for this provider
            let env_var_names = ProviderMetadataService::get_env_var_names(&provider_id);

            // Try to find API key in any of the env vars
            let api_key = env_var_names
                .iter()
                .find_map(|var_name| std::env::var(var_name).ok())
                .filter(|key| !key.is_empty());

            match api_key {
                Some(key) => {
                    // Create and save credential
                    let credential = ProviderCredential::new_api_key(provider_id, key);
                    match self.infra.upsert_credential(credential).await {
                        Ok(_) => summary.imported.push(provider_id),
                        Err(e) => summary.failed.push((provider_id, e.to_string())),
                    }
                }
                None => {
                    // No env var found or all were empty
                    summary.skipped.push(provider_id);
                }
            }
        }

        Ok(summary)
    }
}
