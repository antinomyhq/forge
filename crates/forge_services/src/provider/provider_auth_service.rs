//! Provider authentication service implementation
//!
//! Implements the `ProviderAuthService` trait using the auth flow factory
//! pattern. This service coordinates authentication flows for all provider
//! types including custom user-defined providers.

use std::sync::Arc;
use std::time::Duration;

use forge_app::ProviderAuthService;
use forge_app::dto::{AuthContext, AuthInitiation, AuthResult, ProviderCredential, ProviderId};

use super::auth_flow::{AuthFlow, AuthFlowInfra, AuthenticationFlow};
use crate::infra::{
    AppConfigRepository, EnvironmentInfra, ProviderCredentialRepository,
    ProviderSpecificProcessingInfra,
};
use crate::provider::AuthMethod;

/// Provider authentication service implementation
///
/// Coordinates authentication flows for LLM providers using the factory
/// pattern. Supports all authentication methods: API keys, OAuth device/code
/// flows, OAuth with API key exchange, cloud services, and custom providers.
pub struct ForgeProviderAuthService<I> {
    infra: Arc<I>,
}

impl<I> ForgeProviderAuthService<I> {
    /// Creates a new provider authentication service
    ///
    /// # Arguments
    /// * `infra` - Infrastructure providing OAuth, GitHub Copilot, and
    ///   credential repository
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<I> ProviderAuthService for ForgeProviderAuthService<I>
where
    I: AuthFlowInfra
        + ProviderCredentialRepository
        + EnvironmentInfra
        + AppConfigRepository
        + ProviderSpecificProcessingInfra
        + Send
        + Sync
        + 'static,
{
    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        method: AuthMethod,
    ) -> anyhow::Result<AuthInitiation> {
        // Get URL parameters from provider config
        let url_param_vars = crate::provider::registry::get_provider_config(&provider_id)
            .map(|config| config.url_param_vars.clone())
            .unwrap_or_default();

        // Create appropriate auth flow using factory
        let flow = AuthFlow::try_new(&provider_id, &method, url_param_vars, self.infra.clone())?;

        // Initiate the authentication flow
        flow.initiate().await.map_err(|e| anyhow::anyhow!(e))
    }

    async fn poll_provider_auth(
        &self,
        provider_id: ProviderId,
        context: &AuthContext,
        timeout: Duration,
        method: AuthMethod,
    ) -> anyhow::Result<AuthResult> {
        // Get URL parameters from provider config
        let url_param_vars = crate::provider::registry::get_provider_config(&provider_id)
            .map(|config| config.url_param_vars.clone())
            .unwrap_or_default();

        // Create appropriate auth flow using factory
        let flow = AuthFlow::try_new(&provider_id, &method, url_param_vars, self.infra.clone())?;

        // Poll until complete
        flow.poll_until_complete(context, timeout)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        result: AuthResult,
        method: AuthMethod,
    ) -> anyhow::Result<ProviderCredential> {
        // Get URL parameters from provider config
        let url_param_vars = crate::provider::registry::get_provider_config(&provider_id)
            .map(|config| config.url_param_vars.clone())
            .unwrap_or_default();

        // Create appropriate auth flow using factory
        let flow = AuthFlow::try_new(&provider_id, &method, url_param_vars, self.infra.clone())?;

        // Complete authentication and create credential
        let credential = flow
            .complete(result)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        // Store credential via infrastructure (takes ownership)
        self.infra.upsert_credential(credential.clone()).await?;

        Ok(credential)
    }
}
