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
use std::time::Duration;

use forge_app::dto::{AuthContext, AuthInitiation, AuthMethodType, AuthResult, ProviderCredential};

pub mod api_key;
pub mod error;
pub mod factory;
pub mod oauth_code;
pub mod oauth_device;
pub mod oauth_with_apikey;

pub use api_key::ApiKeyAuthFlow;
pub use error::AuthFlowError;
pub use factory::{AuthFlowFactory, AuthFlowInfra};
pub use oauth_code::OAuthCodeFlow;

mod cloud_service;
pub use cloud_service::CloudServiceAuthFlow;

mod custom_provider;
pub use custom_provider::CustomProviderAuthFlow;
pub use oauth_device::OAuthDeviceFlow;
pub use oauth_with_apikey::OAuthWithApiKeyFlow;

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
