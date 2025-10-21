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
    AuthContext, AuthInitiation, AuthMethod, AuthMethodType, AuthResult, ProviderCredential,
    ProviderId, UrlParameter,
};

use crate::provider::{
    AuthMethodType as AuthMethodTypeInternal, ForgeOAuthService, GitHubCopilotService,
};

pub mod api_key;
pub mod error;
pub mod oauth_code;
pub mod oauth_device;
pub mod oauth_with_apikey;

pub use api_key::ApiKeyAuthFlow;
pub use error::AuthFlowError;
pub use oauth_code::OAuthCodeFlow;

mod cloud_service;
pub use cloud_service::CloudServiceAuthFlow;
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
    /// Cloud service with URL parameters (Vertex AI, Azure)
    CloudService(CloudServiceAuthFlow),
    /// OAuth device code flow
    OAuthDevice(OAuthDeviceFlow),
    /// OAuth authorization code flow
    OAuthCode(OAuthCodeFlow),
    /// OAuth with API key exchange (GitHub Copilot)
    OAuthWithApiKey(OAuthWithApiKeyFlow),
}

impl AuthFlow {
    /// Creates an authentication flow for the specified provider and method
    ///
    /// # Arguments
    /// * `provider_id` - The provider to create a flow for
    /// * `auth_method` - The authentication method configuration
    /// * `infra` - Infrastructure services (OAuth, HTTP, etc.)
    ///
    /// # Returns
    /// An `AuthFlow` enum wrapping the appropriate flow implementation
    ///
    /// # Errors
    /// Returns error if the authentication method type is unsupported or
    /// required configuration is missing
    pub fn try_new<I>(
        provider_id: &ProviderId,
        auth_method: &AuthMethod,
        infra: Arc<I>,
    ) -> anyhow::Result<Self>
    where
        I: AuthFlowInfra + 'static,
    {
        match auth_method.method_type {
            AuthMethodTypeInternal::ApiKey => {
                // Check if this is a cloud provider that needs URL parameters
                let required_params = Self::get_provider_params(provider_id);

                if required_params.is_empty() {
                    // Simple API key authentication
                    Ok(Self::ApiKey(ApiKeyAuthFlow::new(provider_id.clone())))
                } else {
                    // Cloud service with URL parameters
                    let flow = CloudServiceAuthFlow::new(provider_id.clone(), required_params);
                    Ok(Self::CloudService(flow))
                }
            }

            AuthMethodTypeInternal::OAuthDevice => {
                let config = auth_method
                    .oauth_config
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OAuth device flow requires oauth_config"))?;

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

            AuthMethodTypeInternal::OAuthCode => {
                let config = auth_method
                    .oauth_config
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OAuth code flow requires oauth_config"))?;

                Ok(Self::OAuthCode(OAuthCodeFlow::new(
                    provider_id.clone(),
                    config.clone(),
                    infra.oauth_service(),
                )))
            }
        }
    }

    /// Gets required URL parameters for cloud providers
    ///
    /// Returns parameter definitions for providers that require additional
    /// configuration beyond API keys (e.g., Vertex AI project_id, Azure
    /// resource_name).
    fn get_provider_params(provider_id: &ProviderId) -> Vec<UrlParameter> {
        match provider_id {
            ProviderId::VertexAi => Self::vertex_ai_params(),
            ProviderId::Azure => Self::azure_params(),
            ProviderId::OpenAICompatible | ProviderId::AnthropicCompatible => {
                Self::compatible_provider_params()
            }
            _ => vec![],
        }
    }

    /// Returns OpenAI/Anthropic compatible provider required parameters
    fn compatible_provider_params() -> Vec<UrlParameter> {
        vec![
            UrlParameter::required("BASE_URL", "Base URL")
                .description("API endpoint (e.g., http://localhost:8080/v1)")
                .validation_pattern(r"^https?://.+"),
        ]
    }

    /// Returns Vertex AI required parameters
    fn vertex_ai_params() -> Vec<UrlParameter> {
        vec![
            UrlParameter::required("project_id", "GCP Project ID")
                .description("Your Google Cloud project ID")
                .validation_pattern(r"^[a-z][a-z0-9-]{4,28}[a-z0-9]$"),
            UrlParameter::required("location", "Location")
                .description("GCP region (e.g., us-central1) or 'global'")
                .default_value("us-central1"),
        ]
    }

    /// Returns Azure OpenAI required parameters
    fn azure_params() -> Vec<UrlParameter> {
        vec![
            UrlParameter::required("resource_name", "Azure Resource Name")
                .description("Your Azure OpenAI resource name"),
            UrlParameter::required("deployment_name", "Deployment Name")
                .description("Your model deployment name"),
            UrlParameter::required("api_version", "API Version")
                .description("Azure API version")
                .default_value("2024-02-15-preview"),
        ]
    }
}

#[async_trait::async_trait]
impl AuthenticationFlow for AuthFlow {
    fn auth_method_type(&self) -> AuthMethodType {
        match self {
            Self::ApiKey(flow) => flow.auth_method_type(),
            Self::CloudService(flow) => flow.auth_method_type(),
            Self::OAuthDevice(flow) => flow.auth_method_type(),
            Self::OAuthCode(flow) => flow.auth_method_type(),
            Self::OAuthWithApiKey(flow) => flow.auth_method_type(),
        }
    }

    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.initiate().await,
            Self::CloudService(flow) => flow.initiate().await,
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
            Self::CloudService(flow) => flow.poll_until_complete(context, timeout).await,
            Self::OAuthDevice(flow) => flow.poll_until_complete(context, timeout).await,
            Self::OAuthCode(flow) => flow.poll_until_complete(context, timeout).await,
            Self::OAuthWithApiKey(flow) => flow.poll_until_complete(context, timeout).await,
        }
    }

    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.complete(result).await,
            Self::CloudService(flow) => flow.complete(result).await,
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
            Self::CloudService(flow) => flow.refresh(credential).await,
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
    fn auth_method_type(&self) -> AuthMethodType;

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
