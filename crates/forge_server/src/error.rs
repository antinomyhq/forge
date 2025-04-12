use std::fmt;

use serde::Serialize;
use thiserror::Error;

/// Custom error type for the Forge server
#[derive(Error, Debug)]
pub enum Error {
    /// Error from the underlying API
    #[error("API error: {0}")]
    Api(#[from] anyhow::Error),

    /// Error serializing or deserializing JSON
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Error handling HTTP request or response
    #[error("HTTP error: {0}")]
    Http(#[from] http::Error),

    /// Error from IO operations
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Error when a conversation is not found
    #[error("Conversation not found: {0}")]
    ConversationNotFound(String),

    /// Error with a request parameter
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
}

/// Serializable error response
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

/// Result type for the Forge server
pub type Result<T> = std::result::Result<T, Error>;
