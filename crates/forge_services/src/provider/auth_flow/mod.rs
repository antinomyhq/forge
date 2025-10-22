/// Generic authentication flow supporting all provider authentication patterns.
///
/// This module provides a unified trait-based approach to handle:
/// - Simple API key authentication (OpenAI, Anthropic, etc.)
/// - OAuth Device Flow (GitHub standard pattern)
/// - OAuth + API Key Exchange (GitHub Copilot - OAuth token → time-limited API
///   key)
/// - OAuth Authorization Code Flow (Web-based providers)
/// - Cloud Service Account with Parameters (Google Vertex AI, Azure with
///   project/resource parameters)
use std::sync::Arc;
use std::time::Duration;

use forge_app::dto::{
    AuthContext, AuthInitiation, AuthMethod, AuthResult, ProviderCredential, ProviderId,
    UrlParameter,
};

use crate::provider::{ForgeOAuthService, GitHubCopilotService};

pub mod api_key;
pub mod error;
pub mod oauth_code;
pub mod oauth_device;
pub mod oauth_with_apikey;

pub use api_key::ApiKeyAuthFlow;
pub use error::AuthFlowError;
pub use oauth_code::OAuthCodeFlow;
pub use oauth_device::OAuthDeviceFlow;
pub use oauth_with_apikey::OAuthWithApiKeyFlow;

/// Infrastructure requirements for creating authentication flows
///
/// This trait defines the minimal set of services needed to instantiate
/// authentication flows. Implementations should provide access to OAuth
/// services, HTTP clients, and provider-specific services.
pub trait AuthFlowInfra: Send + Sync {
    /// Returns the OAuth service for token operations
    fn oauth_service(&self) -> Arc<ForgeOAuthService>;

    /// Returns the GitHub Copilot service for API key exchange
    fn github_copilot_service(&self) -> Arc<GitHubCopilotService>;
}

/// Authentication flow enum
///
/// This enum wraps all possible authentication flow implementations,
/// eliminating the need for dynamic dispatch while maintaining type safety.
pub enum AuthFlow {
    /// Simple API key authentication
    ApiKey(ApiKeyAuthFlow),

    /// OAuth device code flow
    OAuthDevice(OAuthDeviceFlow),
    /// OAuth authorization code flow
    OAuthCode(OAuthCodeFlow),
    /// OAuth with API key exchange (GitHub Copilot)
    OAuthWithApiKey(OAuthWithApiKeyFlow),
}

impl AuthFlow {
    /// Creates an authentication flow for the specified provider and method
    pub fn try_new<I>(
        provider_id: &ProviderId,
        auth_method: &AuthMethod,
        url_param_vars: Vec<String>,
        infra: Arc<I>,
    ) -> anyhow::Result<Self>
    where
        I: AuthFlowInfra + 'static,
    {
        match auth_method {
            AuthMethod::ApiKey => {
                // Convert url_param_vars to UrlParameters with simple prompts
                let required_params = url_param_vars
                    .into_iter()
                    .map(|var| UrlParameter::required(var.clone(), var.to_string()))
                    .collect();

                Ok(Self::ApiKey(ApiKeyAuthFlow::new(
                    provider_id.clone(),
                    required_params,
                )))
            }

            AuthMethod::OAuthDevice(config) => {
                // Check if this is GitHub Copilot (OAuth with API key exchange)
                if config.token_refresh_url.is_some() {
                    let github_service = infra.github_copilot_service();
                    Ok(Self::OAuthWithApiKey(OAuthWithApiKeyFlow::new(
                        provider_id.clone(),
                        config.clone(),
                        infra.oauth_service(),
                        github_service,
                    )))
                } else {
                    Ok(Self::OAuthDevice(OAuthDeviceFlow::new(
                        provider_id.clone(),
                        config.clone(),
                        infra.oauth_service(),
                    )))
                }
            }

            AuthMethod::OAuthCode(config) => Ok(Self::OAuthCode(OAuthCodeFlow::new(
                provider_id.clone(),
                config.clone(),
                infra.oauth_service(),
            ))),
        }
    }
}

#[async_trait::async_trait]
impl AuthenticationFlow for AuthFlow {
    fn auth_method_type(&self) -> AuthMethod {
        match self {
            Self::ApiKey(flow) => flow.auth_method_type(),

            Self::OAuthDevice(flow) => flow.auth_method_type(),
            Self::OAuthCode(flow) => flow.auth_method_type(),
            Self::OAuthWithApiKey(flow) => flow.auth_method_type(),
        }
    }

    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.initiate().await,

            Self::OAuthDevice(flow) => flow.initiate().await,
            Self::OAuthCode(flow) => flow.initiate().await,
            Self::OAuthWithApiKey(flow) => flow.initiate().await,
        }
    }

    async fn poll_until_complete(
        &self,
        context: &AuthContext,
        timeout: Duration,
    ) -> Result<AuthResult, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.poll_until_complete(context, timeout).await,

            Self::OAuthDevice(flow) => flow.poll_until_complete(context, timeout).await,
            Self::OAuthCode(flow) => flow.poll_until_complete(context, timeout).await,
            Self::OAuthWithApiKey(flow) => flow.poll_until_complete(context, timeout).await,
        }
    }

    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.complete(result).await,

            Self::OAuthDevice(flow) => flow.complete(result).await,
            Self::OAuthCode(flow) => flow.complete(result).await,
            Self::OAuthWithApiKey(flow) => flow.complete(result).await,
        }
    }

    async fn refresh(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.refresh(credential).await,

            Self::OAuthDevice(flow) => flow.refresh(credential).await,
            Self::OAuthCode(flow) => flow.refresh(credential).await,
            Self::OAuthWithApiKey(flow) => flow.refresh(credential).await,
        }
    }
}

/// Generic authentication flow trait supporting all provider auth patterns.
#[async_trait::async_trait]
pub trait AuthenticationFlow: Send + Sync {
    /// Returns the authentication method type.
    fn auth_method_type(&self) -> AuthMethod;

    /// Initiates the authentication flow.
    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError>;

    /// Polls until authentication completes or times out.
    async fn poll_until_complete(
        &self,
        context: &AuthContext,
        timeout: Duration,
    ) -> Result<AuthResult, AuthFlowError>;

    /// Completes the authentication flow.
    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError>;

    /// Refreshes expired credentials.
    async fn refresh(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError>;
}
