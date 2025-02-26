use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

/// Custom error type for the application
#[derive(Debug, Error)]
pub enum Error {
    /// Authentication or authorization error
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Database operation error
    #[error("Database error: {0}")]
    Database(String),

    /// Service error
    #[error("Service error: {0}")]
    Service(String),

    /// Bad requests
    #[error("{0}")]
    BadRequest(#[from] validator::ValidationErrors),

    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Not found error
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias using our custom Error
pub type Result<T> = std::result::Result<T, Error>;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Error::Auth(msg) => (StatusCode::UNAUTHORIZED, msg),
            Error::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            Error::Service(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            Error::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.to_string()),
            Error::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Error::Serde(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            Error::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(json!({
            "error": {
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}

impl From<forge_domain::Error> for Error {
    fn from(err: forge_domain::Error) -> Self {
        Error::Service(err.to_string())
    }
}
