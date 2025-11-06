use std::time::Duration;

/// Authentication flow errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// Authentication initiation failed.
    #[error("Authentication initiation failed: {0}")]
    InitiationFailed(String),

    /// Authentication timed out waiting for user.
    #[error("Authentication timed out after {0:?}")]
    Timeout(Duration),

    /// Device code or session expired before completion.
    #[error("Device code or session expired")]
    Expired,

    /// User denied authorization request.
    #[error("User denied authorization")]
    Denied,

    /// Polling operation failed due to network or server error.
    #[error("Polling failed: {0}")]
    PollFailed(String),

    /// Authentication completion failed.
    #[error("Authentication completion failed: {0}")]
    CompletionFailed(String),

    /// Token refresh operation failed.
    #[error("Token refresh failed: {0}")]
    RefreshFailed(String),

    /// Invalid authentication context for the flow type.
    #[error("Invalid context: {0}")]
    InvalidContext(String),
}
