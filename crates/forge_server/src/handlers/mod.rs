mod app_state;
mod chat;
mod conversation;
mod environment;
mod init;
mod load;
mod models;
mod suggestions;
mod tools;
mod upsert;
mod variable;

pub use app_state::*;
use axum::response::{IntoResponse, Response};
use axum::Json;
pub use chat::*;
pub use conversation::*;
pub use environment::*;
use http::StatusCode;
pub use init::*;
pub use load::*;
pub use models::*;
pub use suggestions::*;
pub use tools::*;
pub use upsert::*;
pub use variable::*;

// Re-export the IntoResponse implementation
use crate::error::{Error, ErrorResponse};

// Convert errors to HTTP responses
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            Error::ConversationNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            Error::InvalidParameter(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = Json(ErrorResponse { error: error_message });

        (status, body).into_response()
    }
}
