/// Authentication flow errors.
use std::time::Duration;

/// Errors that can occur during authentication flows.
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

    /// Credential validation failed.
    #[error("Credential validation failed: {0}")]
    ValidationFailed(String),

    /// Required parameter is missing.
    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    /// Parameter value is invalid.
    #[error("Invalid parameter value for '{0}': {1}")]
    InvalidParameter(String, String),

    /// Base URL is invalid or malformed.
    #[error("Invalid base URL: {0}")]
    InvalidBaseUrl(String),

    /// Custom provider validation failed.
    #[error("Custom provider validation failed: {0}")]
    CustomProviderValidationFailed(String),

    /// Invalid authentication context for the flow type.
    #[error("Invalid context: {0}")]
    InvalidContext(String),

    /// No valid source files found to index.
    #[error("No valid source files found to index")]
    NoSourceFilesFound,
}
