/// Infrastructure requirements for provider authentication operations.
///
/// This trait defines the minimal set of services needed for authentication
/// flows. Implementations should provide access to OAuth services.
use std::sync::Arc;

use crate::provider::ForgeOAuthService;

/// Infrastructure requirements for authentication flows
///
/// This trait defines the minimal set of services needed to perform
/// authentication operations. Implementations should provide access to OAuth
/// services and HTTP clients.
pub trait AuthFlowInfra: Send + Sync {
    /// Returns the OAuth service for token operations
    fn oauth_service(&self) -> Arc<ForgeOAuthService>;
}
