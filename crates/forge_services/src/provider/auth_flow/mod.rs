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
/// - Custom Provider Registration (User-defined OpenAI-compatible and
///   Anthropic-compatible providers)
use std::sync::Arc;
use std::time::Duration;

use forge_app::dto::{
    AuthContext, AuthInitiation, AuthMethodType, AuthResult, ProviderId, ProviderCredential,
    ProviderResponse, UrlParameter,
};

use crate::provider::{AuthMethod, AuthMethodType as AuthMethodTypeInternal, ForgeOAuthService, GitHubCopilotService};

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

mod custom_provider;
pub use custom_provider::CustomProviderAuthFlow;
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
    /// Custom provider authentication
    CustomProvider(CustomProviderAuthFlow),
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
                    Ok(Self::ApiKey(ApiKeyAuthFlow::new(
                        provider_id.clone(),
                        auth_method.label.clone(),
                        auth_method.description.clone(),
                    )))
                } else {
                    // Cloud service with URL parameters
                    let flow = CloudServiceAuthFlow::new(
                        provider_id.clone(),
                        required_params,
                        auth_method.label.clone(),
                    );
                    let flow = if let Some(desc) = &auth_method.description {
                        flow.with_description(desc)
                    } else {
                        flow
                    };
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

    /// Creates a custom provider authentication flow
    ///
    /// Custom providers use a separate flow that prompts for provider-specific
    /// configuration (base URL, model ID, compatibility mode).
    ///
    /// # Arguments
    /// * `compatibility_mode` - Whether the provider is OpenAI or Anthropic
    ///   compatible
    pub fn new_custom_provider(compatibility_mode: ProviderResponse) -> Self {
        Self::CustomProvider(CustomProviderAuthFlow::new(compatibility_mode))
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
            _ => vec![],
        }
    }

    /// Returns Vertex AI required parameters
    fn vertex_ai_params() -> Vec<UrlParameter> {
        vec![
            UrlParameter::new("project_id", "GCP Project ID")
                .with_description("Your Google Cloud project ID")
                .with_required(true)
                .with_validation_pattern(r"^[a-z][a-z0-9-]{4,28}[a-z0-9]$"),
            UrlParameter::new("location", "Location")
                .with_description("GCP region (e.g., us-central1) or 'global'")
                .with_default_value("us-central1")
                .with_required(true),
        ]
    }

    /// Returns Azure OpenAI required parameters
    fn azure_params() -> Vec<UrlParameter> {
        vec![
            UrlParameter::new("resource_name", "Azure Resource Name")
                .with_description("Your Azure OpenAI resource name")
                .with_required(true),
            UrlParameter::new("deployment_name", "Deployment Name")
                .with_description("Your model deployment name")
                .with_required(true),
            UrlParameter::new("api_version", "API Version")
                .with_description("Azure API version")
                .with_default_value("2024-02-15-preview")
                .with_required(true),
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
            Self::CustomProvider(flow) => flow.auth_method_type(),
        }
    }

    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.initiate().await,
            Self::CloudService(flow) => flow.initiate().await,
            Self::OAuthDevice(flow) => flow.initiate().await,
            Self::OAuthCode(flow) => flow.initiate().await,
            Self::OAuthWithApiKey(flow) => flow.initiate().await,
            Self::CustomProvider(flow) => flow.initiate().await,
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
            Self::CustomProvider(flow) => flow.poll_until_complete(context, timeout).await,
        }
    }

    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.complete(result).await,
            Self::CloudService(flow) => flow.complete(result).await,
            Self::OAuthDevice(flow) => flow.complete(result).await,
            Self::OAuthCode(flow) => flow.complete(result).await,
            Self::OAuthWithApiKey(flow) => flow.complete(result).await,
            Self::CustomProvider(flow) => flow.complete(result).await,
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
            Self::CustomProvider(flow) => flow.refresh(credential).await,
        }
    }

    async fn validate(&self, credential: &ProviderCredential) -> Result<bool, AuthFlowError> {
        match self {
            Self::ApiKey(flow) => flow.validate(credential).await,
            Self::CloudService(flow) => flow.validate(credential).await,
            Self::OAuthDevice(flow) => flow.validate(credential).await,
            Self::OAuthCode(flow) => flow.validate(credential).await,
            Self::OAuthWithApiKey(flow) => flow.validate(credential).await,
            Self::CustomProvider(flow) => flow.validate(credential).await,
        }
    }
}


/// Generic authentication flow trait supporting all provider auth patterns.
///
/// This trait provides a simple, focused interface for authentication flows.
/// The core polling method `poll_until_complete` is intentionally blocking
/// to keep the trait simple - UIs can add their own progress tracking by
/// wrapping the poll call in their own task.
///
/// # Example: Adding Progress Tracking
///
/// ```ignore
/// use std::time::Instant;
/// use tokio::time::sleep;
///
/// let start = Instant::now();
/// let progress_task = tokio::spawn(async move {
///     loop {
///         let elapsed = start.elapsed();
///         println!("Waiting for auth... {:?}", elapsed);
///         sleep(Duration::from_secs(1)).await;
///     }
/// });
///
/// let result = flow.poll_until_complete(context, timeout).await?;
/// progress_task.abort(); // Stop progress tracking
/// ```
#[async_trait::async_trait]
pub trait AuthenticationFlow: Send + Sync {
    /// Returns the authentication method type.
    fn auth_method_type(&self) -> AuthMethodType;

    /// Initiates the authentication flow.
    ///
    /// Returns display information for the user (if interactive).
    /// For providers requiring parameters (Vertex AI, Azure, Custom Providers),
    /// returns `ApiKeyPrompt` with `required_params`.
    ///
    /// # Errors
    ///
    /// Returns `AuthFlowError::InitiationFailed` if the flow cannot be started.
    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError>;

    /// Polls until authentication completes or times out.
    ///
    /// This is a blocking async function that handles all polling internally.
    /// For non-pollable flows (manual API key entry, authorization code),
    /// this method returns an error immediately.
    ///
    /// # Arguments
    ///
    /// * `context` - Context data from initiation (device code, session ID,
    ///   etc.)
    /// * `timeout` - Maximum duration to wait for completion
    ///
    /// # Returns
    ///
    /// * `Ok(AuthResult)` - Authentication completed successfully
    /// * `Err(AuthFlowError::Timeout)` - Timed out waiting for user
    /// * `Err(AuthFlowError::Expired)` - Device code/session expired
    /// * `Err(AuthFlowError::Denied)` - User denied authorization
    /// * `Err(AuthFlowError::PollFailed)` - Network or server error
    ///
    /// # Note for UI Progress
    ///
    /// If you need progress updates, wrap this in your own task and track
    /// elapsed time. See the trait-level documentation for an example.
    ///
    /// # Errors
    ///
    /// Returns various `AuthFlowError` variants depending on the failure mode.
    async fn poll_until_complete(
        &self,
        context: &AuthContext,
        timeout: Duration,
    ) -> Result<AuthResult, AuthFlowError>;

    /// Completes the authentication flow.
    ///
    /// Processes final tokens/credentials and returns credential.
    /// For cloud providers and custom providers, uses `url_params` from
    /// `AuthResult::ApiKey`.
    ///
    /// # Errors
    ///
    /// Returns `AuthFlowError::CompletionFailed` if credentials cannot be
    /// created.
    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError>;

    /// Refreshes expired credentials.
    ///
    /// Returns updated credential with fresh tokens.
    ///
    /// # Errors
    ///
    /// Returns `AuthFlowError::RefreshFailed` if the refresh operation fails.
    async fn refresh(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError>;

    /// Validates if credentials are still valid.
    ///
    /// # Errors
    ///
    /// Returns `AuthFlowError::ValidationFailed` if validation cannot be
    /// performed.
    async fn validate(&self, credential: &ProviderCredential) -> Result<bool, AuthFlowError>;
}
