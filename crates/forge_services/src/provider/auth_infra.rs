/// Infrastructure requirements for provider authentication operations.
///
/// This trait defines the minimal set of services needed for authentication
/// flows. Implementations should provide access to OAuth services and
/// provider-specific services like GitHub Copilot.
use std::sync::Arc;

use crate::provider::{ForgeOAuthService, GitHubCopilotService};

/// Infrastructure requirements for authentication flows
///
/// This trait defines the minimal set of services needed to perform
/// authentication operations. Implementations should provide access to OAuth
/// services, HTTP clients, and provider-specific services.
pub trait AuthFlowInfra: Send + Sync {
    /// Returns the OAuth service for token operations
    fn oauth_service(&self) -> Arc<ForgeOAuthService>;

    /// Returns the GitHub Copilot service for API key exchange
    fn github_copilot_service(&self) -> Arc<GitHubCopilotService>;
}
