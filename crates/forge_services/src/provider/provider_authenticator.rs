/// DTOs for provider authentication
#[derive(Debug, Clone)]
pub struct OAuthDeviceInit {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub state: OAuthDeviceState,
}

#[derive(Debug, Clone)]
pub struct OAuthDeviceState {
    pub device_auth_response: super::oauth::DeviceAuthorizationResponse,
    pub oauth_config: super::OAuthConfig,
    pub provider_id: ProviderId,
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

use chrono::Utc;
use forge_app::ProviderRegistry;
use forge_app::dto::{OAuthTokens, ProviderCredential, ProviderId};

use super::{
    ForgeOAuthService, ForgeProviderValidationService, GitHubCopilotService,
    ProviderMetadataService, ValidationResult,
};
use crate::infra::ProviderCredentialRepository;

/// Provider authenticator - coordinates provider authentication flows
///
/// This struct handles all provider authentication operations including:
/// - Adding API key credentials with validation
/// - OAuth device flow (initiate and complete)
/// - Importing credentials from environment variables
///
/// It uses `ProviderMetadataService` to get OAuth configurations and
/// environment variable names, ensuring no hardcoded configuration in the
/// authentication logic.
pub struct ProviderAuthenticator<S> {
    services: Arc<S>,
}

impl<S> ProviderAuthenticator<S>
where
    S: ProviderCredentialRepository + ProviderRegistry + crate::infra::HttpInfra + Send + Sync,
{
    /// Creates a new provider authenticator
    ///
    /// # Arguments
    ///
    /// * `services` - Services providing credential storage and provider
    ///   registry
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
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
            let validation_service =
                ForgeProviderValidationService::new(Arc::clone(&self.services));
            let result = validation_service
                .validate_credential(&provider_id, &credential, &provider.model_url)
                .await?;

            match result {
                ValidationResult::Valid => {
                    // Save and return success
                    self.services.upsert_credential(credential).await?;
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
                    self.services.upsert_credential(credential).await?;
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
            self.services.upsert_credential(credential).await?;
            Ok(ValidationOutcome::success_with_message(
                "API key saved without validation",
            ))
        }
    }

    /// Initiate OAuth device authorization flow
    ///
    /// Gets OAuth configuration from provider metadata and initiates the device
    /// authorization flow. Returns display information for the user and opaque
    /// state to complete the flow.
    ///
    /// # Arguments
    ///
    /// * `provider_id` - Provider to authenticate with
    ///
    /// # Returns
    ///
    /// OAuth device init containing user instructions and completion state
    ///
    /// # Errors
    ///
    /// Returns error if provider doesn't support OAuth or initiation fails
    pub async fn initiate_oauth_device(
        &self,
        provider_id: ProviderId,
    ) -> anyhow::Result<OAuthDeviceInit> {
        // Get OAuth config from metadata service
        let auth_method = ProviderMetadataService::get_oauth_method(&provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider {} does not support OAuth", provider_id))?;

        let oauth_config = auth_method
            .oauth_config
            .ok_or_else(|| anyhow::anyhow!("OAuth config missing"))?;

        // Call OAuth service with metadata config
        let oauth_service = ForgeOAuthService::new();
        let device_response = oauth_service.initiate_device_auth(&oauth_config).await?;

        // Return display info + opaque state
        Ok(OAuthDeviceInit {
            user_code: device_response.user_code.clone(),
            verification_uri: device_response.verification_uri.clone(),
            expires_in: device_response.expires_in,
            state: OAuthDeviceState {
                device_auth_response: device_response,
                oauth_config,
                provider_id,
            },
        })
    }

    /// Complete OAuth device authorization flow
    ///
    /// Polls for authorization completion, performs provider-specific
    /// post-processing (e.g., fetching Copilot API key), and saves the
    /// credential.
    ///
    /// This method BLOCKS until the user completes authorization.
    ///
    /// # Arguments
    ///
    /// * `state` - Opaque state from initiate_oauth_device
    ///
    /// # Errors
    ///
    /// Returns error if polling fails, authorization is denied, or credential
    /// save fails
    pub async fn complete_oauth_device(&self, state: OAuthDeviceState) -> anyhow::Result<()> {
        let oauth_service = ForgeOAuthService::new();

        // Poll until authorized (BLOCKING)
        let oauth_tokens = oauth_service
            .poll_device_auth(&state.oauth_config, &state.device_auth_response)
            .await?;

        // Provider-specific post-processing
        let credential = self
            .create_oauth_credential(state.provider_id, oauth_tokens, &state.oauth_config)
            .await?;

        // Save credential
        self.services.upsert_credential(credential).await?;

        Ok(())
    }

    /// Create OAuth credential with provider-specific logic
    ///
    /// Handles provider-specific post-processing like fetching Copilot API
    /// keys.
    ///
    /// # Arguments
    ///
    /// * `provider_id` - Provider being authenticated
    /// * `oauth_tokens` - OAuth tokens from authorization
    /// * `oauth_config` - OAuth configuration (for token refresh URLs, etc.)
    ///
    /// # Errors
    ///
    /// Returns error if provider-specific processing fails
    async fn create_oauth_credential(
        &self,
        provider_id: ProviderId,
        oauth_tokens: super::OAuthTokenResponse,
        _oauth_config: &super::OAuthConfig,
    ) -> anyhow::Result<ProviderCredential> {
        match provider_id {
            ProviderId::GitHubCopilot => {
                // GitHub Copilot: Exchange OAuth token for API key
                let copilot_service = GitHubCopilotService::new();
                let (api_key, expires_at) = copilot_service
                    .get_copilot_api_key(&oauth_tokens.access_token)
                    .await?;

                Ok(ProviderCredential::new_oauth_with_api_key(
                    provider_id,
                    api_key,
                    OAuthTokens {
                        access_token: oauth_tokens.access_token.clone(),
                        refresh_token: oauth_tokens.access_token, // Use same token for refresh
                        expires_at,
                    },
                ))
            }
            _ => {
                // Generic OAuth credential
                let expires_at = oauth_tokens
                    .expires_in
                    .map(|secs| Utc::now() + chrono::Duration::seconds(secs as i64))
                    .unwrap_or_else(|| Utc::now() + chrono::Duration::days(365));

                Ok(ProviderCredential::new_oauth(
                    provider_id,
                    OAuthTokens {
                        access_token: oauth_tokens.access_token,
                        refresh_token: oauth_tokens.refresh_token.unwrap_or_default(),
                        expires_at,
                    },
                ))
            }
        }
    }

    /// Import credentials from environment variables
    ///
    /// Scans environment for credentials using provider metadata to determine
    /// which environment variables to check. Validates and imports found
    /// credentials.
    ///
    /// # Arguments
    ///
    /// * `filter` - Optional provider ID to import only that provider
    ///
    /// # Returns
    ///
    /// Summary of import operation (imported, failed, skipped)
    ///
    /// # Errors
    ///
    /// Returns error if provider list cannot be retrieved
    pub async fn import_from_environment(
        &self,
        filter: Option<ProviderId>,
    ) -> anyhow::Result<ImportSummary> {
        let mut summary = ImportSummary::default();

        // Get available provider IDs from configuration
        let available_ids = self.services.available_provider_ids();

        for provider_id in available_ids {
            // Apply filter if specified
            if let Some(ref filter_id) = filter
                && &provider_id != filter_id
            {
                continue;
            }

            // Check if already configured
            if self.services.get_credential(&provider_id).await?.is_some() {
                summary.skipped.push(provider_id);
                continue;
            }

            // Get env var names from metadata
            let env_var_names = ProviderMetadataService::get_env_var_names(&provider_id);

            // Try each env var
            let api_key = env_var_names
                .iter()
                .find_map(|var_name| std::env::var(var_name).ok());

            if let Some(api_key) = api_key {
                // Validate and import
                match self
                    .add_api_key_credential(provider_id, api_key, false)
                    .await
                {
                    Ok(outcome) => {
                        if outcome.success {
                            summary.imported.push(provider_id);
                        } else {
                            summary.failed.push((
                                provider_id,
                                outcome
                                    .message
                                    .unwrap_or_else(|| "Validation failed".to_string()),
                            ));
                        }
                    }
                    Err(e) => summary.failed.push((provider_id, e.to_string())),
                }
            }
        }

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_validation_outcome_construction() {
        let success = ValidationOutcome::success();
        assert_eq!(success.success, true);

        let failure = ValidationOutcome::failure("Invalid key");
        assert_eq!(failure.success, false);
        assert_eq!(failure.message, Some("Invalid key".to_string()));
    }

    #[test]
    fn test_import_summary_tracking() {
        let mut summary = ImportSummary::new();
        assert_eq!(summary.total_processed(), 0);

        summary.imported.push(ProviderId::OpenAI);
        summary.skipped.push(ProviderId::Anthropic);
        summary.failed.push((ProviderId::Xai, "Error".to_string()));

        assert_eq!(summary.total_processed(), 3);
        assert_eq!(summary.has_imports(), true);
        assert_eq!(summary.has_failures(), true);
    }
}
