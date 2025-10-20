use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use forge_domain::RetryConfig;

use crate::dto::{
    AuthContext, AuthInitiation, AuthResult, CompatibilityMode, InitAuth, ProviderId,
};
use crate::{AuthService, Error, ProviderAuthService};

/// Authenticator handles both Forge platform authentication and provider
/// authentication
///
/// Supports two authentication flows:
/// 1. **Forge Platform Auth**: init() → login() → logout() for Forge API access
/// 2. **Provider Auth**: authenticate_provider() for LLM provider credentials
pub struct Authenticator<S, P> {
    auth_service: Arc<S>,
    provider_auth_service: Arc<P>,
}

impl<S: AuthService, P: ProviderAuthService> Authenticator<S, P> {
    /// Creates a new authenticator with both platform and provider auth
    /// services
    ///
    /// # Arguments
    /// * `auth_service` - Service for Forge platform authentication
    /// * `provider_auth_service` - Service for LLM provider authentication
    pub fn new(auth_service: Arc<S>, provider_auth_service: Arc<P>) -> Self {
        Self { auth_service, provider_auth_service }
    }

    // ========== Forge Platform Authentication ==========

    /// Initializes Forge platform authentication
    ///
    /// Returns device code information for user to authorize in browser
    pub async fn init(&self) -> anyhow::Result<InitAuth> {
        self.auth_service.init_auth().await
    }

    /// Polls until user completes Forge platform authentication
    ///
    /// This blocks until the user authorizes the device code in their browser
    /// or the timeout is reached.
    pub async fn login(&self, init_auth: &InitAuth) -> anyhow::Result<()> {
        self.poll(
            RetryConfig::default()
                .max_retry_attempts(300usize)
                .max_delay(2)
                .backoff_factor(1u64),
            || self.login_inner(init_auth),
        )
        .await
    }

    /// Logs out of Forge platform by clearing stored credentials
    pub async fn logout(&self) -> anyhow::Result<()> {
        self.auth_service.set_auth_token(None).await?;
        Ok(())
    }

    async fn login_inner(&self, init_auth: &InitAuth) -> anyhow::Result<()> {
        let key_info = self.auth_service.get_auth_token().await?;
        if key_info.is_some() {
            return Ok(());
        }
        let key = self.auth_service.login(init_auth).await?;
        self.auth_service.set_auth_token(Some(key)).await?;
        Ok(())
    }

    async fn poll<T, F>(
        &self,
        config: RetryConfig,
        call: impl Fn() -> F + Send,
    ) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>> + Send,
    {
        let mut builder = ExponentialBuilder::default()
            .with_factor(1.0)
            .with_factor(config.backoff_factor as f32)
            .with_max_times(config.max_retry_attempts)
            .with_jitter();
        if let Some(max_delay) = config.max_delay {
            builder = builder.with_max_delay(Duration::from_secs(max_delay))
        }

        call.retry(builder)
            .when(|e| {
                // Only retry on Error::AuthInProgress (202 status)
                e.downcast_ref::<Error>()
                    .map(|v| matches!(v, Error::AuthInProgress))
                    .unwrap_or(false)
            })
            .await
    }

    // ========== Provider Authentication (Low-Level Primitives) ==========

    /// Initiates authentication for an LLM provider
    ///
    /// Returns the initial authentication state which varies by provider:
    /// - API Key providers: Prompts for key input (and optional URL parameters)
    /// - OAuth Device Flow: Returns user code and verification URL
    /// - OAuth Code Flow: Returns authorization URL for browser redirect
    /// - Custom Providers: Prompts for base URL, model ID, compatibility mode
    ///
    /// # Arguments
    /// * `provider_id` - The provider to authenticate (e.g., OpenAI,
    ///   GithubCopilot)
    ///
    /// # Returns
    /// `AuthInitiation` enum with provider-specific instructions for the UI
    ///
    /// # Example
    /// ```ignore
    /// let initiation = authenticator.init_provider_auth(ProviderId::OpenAI).await?;
    /// match initiation {
    ///     AuthInitiation::ApiKeyPrompt { label, .. } => {
    ///         // Display prompt to user
    ///     }
    ///     AuthInitiation::DeviceFlow { user_code, verification_uri, .. } => {
    ///         // Display code and URL to user
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
    ) -> anyhow::Result<AuthInitiation> {
        self.provider_auth_service
            .init_provider_auth(provider_id)
            .await
    }

    /// Polls until provider authentication completes
    ///
    /// This is a blocking async operation that waits for authentication to
    /// complete. For OAuth flows, it polls the token endpoint. For manual
    /// input (API keys), this should not be called.
    ///
    /// # Arguments
    /// * `provider_id` - The provider being authenticated
    /// * `context` - Context data from initiation (device code, session ID,
    ///   etc.)
    /// * `timeout` - Maximum duration to wait for completion
    ///
    /// # Returns
    /// `AuthResult` containing the authentication outcome (tokens, API key,
    /// etc.)
    pub async fn poll_provider_auth(
        &self,
        provider_id: ProviderId,
        context: &AuthContext,
        timeout: Duration,
    ) -> anyhow::Result<AuthResult> {
        self.provider_auth_service
            .poll_provider_auth(provider_id, context, timeout)
            .await
    }

    /// Completes provider authentication and saves credentials
    ///
    /// Takes the authentication result and creates a provider credential,
    /// then saves it to the database.
    ///
    /// # Arguments
    /// * `provider_id` - The provider being authenticated
    /// * `result` - The authentication result from user input or polling
    ///
    /// # Returns
    /// The created and saved `ProviderCredential`
    pub async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        result: AuthResult,
    ) -> anyhow::Result<crate::dto::ProviderCredential> {
        self.provider_auth_service
            .complete_provider_auth(provider_id, result)
            .await
    }

    // ========== Custom Provider Management ==========

    /// Initiates custom provider registration
    ///
    /// Returns prompts for user to provide custom provider details:
    /// - Provider name
    /// - Base URL
    /// - Model ID
    /// - API key (optional for local servers)
    ///
    /// # Arguments
    /// * `compatibility_mode` - OpenAI or Anthropic API compatibility
    pub async fn init_custom_provider_auth(
        &self,
        compatibility_mode: CompatibilityMode,
    ) -> anyhow::Result<AuthInitiation> {
        self.provider_auth_service
            .init_custom_provider(compatibility_mode)
            .await
    }

    /// Registers a custom provider with the provided configuration
    ///
    /// # Arguments
    /// * `result` - AuthResult::CustomProvider with all configuration
    ///
    /// # Returns
    /// The generated ProviderId for the custom provider
    pub async fn register_custom_provider(&self, result: AuthResult) -> anyhow::Result<ProviderId> {
        self.provider_auth_service
            .register_custom_provider(result)
            .await
    }

    /// Lists all registered custom providers
    pub async fn list_custom_providers(
        &self,
    ) -> anyhow::Result<Vec<crate::dto::ProviderCredential>> {
        self.provider_auth_service.list_custom_providers().await
    }

    /// Deletes a custom provider
    ///
    /// # Arguments
    /// * `provider_id` - The custom provider to delete (must be
    ///   ProviderId::Custom)
    pub async fn delete_custom_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.provider_auth_service
            .delete_custom_provider(provider_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_poll_retry_condition() {
        // Test that the retry condition only matches AuthInProgress errors
        let auth_in_progress_error = anyhow::Error::from(Error::AuthInProgress);
        let other_error = anyhow::anyhow!("Some other error");
        let serde_error = anyhow::Error::from(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "test",
        )));

        // Create a test closure that mimics the retry condition
        let retry_condition = |e: &anyhow::Error| {
            if let Some(app_error) = e.downcast_ref::<Error>() {
                matches!(app_error, Error::AuthInProgress)
            } else {
                false
            }
        };

        // Test cases
        assert_eq!(retry_condition(&auth_in_progress_error), true);
        assert_eq!(retry_condition(&other_error), false);
        assert_eq!(retry_condition(&serde_error), false);
    }
}
