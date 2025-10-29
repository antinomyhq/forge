use std::sync::Arc;

use anyhow::Context;

use crate::dto::{AuthMethod, AuthType, Provider};
use crate::{Error, ProviderAuthService, ProviderRegistry};

/// App-level orchestrator for provider operations
///
/// Handles complex provider workflows that require coordination between
/// multiple services without creating service-to-service dependencies.
/// Similar to `Authenticator` which handles authentication orchestration.
pub struct ProviderAuthenticator<S> {
    services: Arc<S>,
}

impl<S> ProviderAuthenticator<S>
where
    S: ProviderRegistry + ProviderAuthService,
{
    /// Creates a new provider orchestrator
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Gets the active provider, automatically refreshing OAuth tokens if
    /// needed
    pub async fn authorized_providers(&self) -> anyhow::Result<Provider> {
        let provider = self.services.get_active_provider().await?;

        // Check if OAuth tokens need refresh
        if let Some(ref credential) = provider.credential
            && credential.needs_token_refresh()
        {
            tracing::debug!(provider = ?provider.id, "OAuth tokens need refresh");

            // Attempt to refresh tokens
            return self
                .refresh_provider_tokens(provider)
                .await
                .with_context(|| "Failed to refresh token");
        }

        Ok(provider)
    }

    /// Refreshes OAuth tokens for a provider
    async fn refresh_provider_tokens(&self, mut provider: Provider) -> anyhow::Result<Provider> {
        let credential = provider
            .credential
            .as_ref()
            .ok_or(Error::NoCredentialToRefresh)?;

        // Determine auth method from credential and provider config
        let auth_method = match credential.auth_type {
            AuthType::OAuth | AuthType::OAuthWithApiKey => {
                // Get auth methods from registry
                let methods = self
                    .services
                    .get_all_providers()
                    .await?
                    .into_iter()
                    .find(|p| p.id == provider.id)
                    .map(|p| p.auth_methods)
                    .unwrap_or_default();

                methods
                    .into_iter()
                    .find(|m| matches!(m, AuthMethod::OAuthDevice(_) | AuthMethod::OAuthCode(_)))
                    .ok_or(Error::NoOAuthMethod(provider.id))?
            }
            AuthType::ApiKey => {
                // API keys don't need refresh
                return Ok(provider.clone());
            }
        };

        // Refresh credential using auth service
        let credentials = self
            .services
            .refresh_provider_credential(&provider, auth_method)
            .await?;

        // Return updated provider with refreshed credential
        provider.credential = Some(credentials.clone());

        // Update key if needed based on auth type
        provider.key = match credentials.auth_type {
            AuthType::OAuth => credentials
                .oauth_tokens
                .as_ref()
                .map(|tokens| tokens.access_token.as_str().to_string().into()),
            _ => provider.key,
        };

        Ok(provider)
    }
}
