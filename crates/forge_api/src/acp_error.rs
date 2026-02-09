//! Error types for ACP integration.
//!
//! This module contains presentation-layer error types for ACP protocol
//! handling.

use agent_client_protocol as acp;

/// Result type for ACP operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during ACP operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error from the ACP protocol layer.
    #[error("ACP protocol error: {0}")]
    Protocol(#[from] acp::Error),

    /// Error from Forge application layer.
    #[error("Forge application error: {0}")]
    Application(#[from] anyhow::Error),

    /// Session not found.
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    /// Invalid request.
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<Error> for acp::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Protocol(e) => e,
            Error::SessionNotFound(msg) => {
                acp::Error::invalid_params().data(format!("Session not found: {msg}"))
            }
            Error::InvalidRequest(msg) => acp::Error::invalid_params().data(msg),
            Error::Application(e) => {
                acp::Error::into_internal_error(e.as_ref() as &dyn std::error::Error)
            }
            Error::Io(e) => acp::Error::into_internal_error(&e),
        }
    }
}
