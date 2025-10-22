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

    fn create_auth_flow(
        &self,
        provider_id: &ProviderId,
        method: &AuthMethod,
    ) -> anyhow::Result<AuthFlow>
    where
        I: AuthFlowInfra + 'static,
    {
        let url_param_vars = crate::provider::registry::get_provider_config(provider_id)
            .map(|config| config.url_param_vars.clone())
            .unwrap_or_default();

        AuthFlow::try_new(provider_id, method, url_param_vars, self.infra.clone())
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
        let flow = self.create_auth_flow(&provider_id, &method)?;
        Ok(flow.initiate().await?)
    }

    async fn poll_provider_auth(
        &self,
        provider_id: ProviderId,
        context: &AuthContext,
        timeout: Duration,
        method: AuthMethod,
    ) -> anyhow::Result<AuthResult> {
        let flow = self.create_auth_flow(&provider_id, &method)?;
        Ok(flow.poll_until_complete(context, timeout).await?)
    }

    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        result: AuthResult,
        method: AuthMethod,
    ) -> anyhow::Result<ProviderCredential> {
        let flow = self.create_auth_flow(&provider_id, &method)?;
        let credential = flow.complete(result).await?;
        self.infra.upsert_credential(credential.clone()).await?;
        Ok(credential)
    }
}
