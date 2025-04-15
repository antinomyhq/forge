//! Error handling for JSON-RPC

use anyhow::Error as AnyhowError;
use jsonrpc_core::{Error as JsonRpcError, ErrorCode};

/// Error codes for custom application errors
pub mod error_codes {
    // Standard JSON-RPC errors
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    pub const SERVER_ERROR_START: i64 = -32000;
    pub const SERVER_ERROR_END: i64 = -32099;

    // Custom error codes (must not clash with standard ones)
    pub const APPLICATION_ERROR: i64 = -32100;
    pub const INVALID_STREAM: i64 = -32101;
    pub const STREAM_CLOSED: i64 = -32102;
    pub const AUTHENTICATION_ERROR: i64 = -32103;
    pub const AUTHORIZATION_ERROR: i64 = -32104;
    pub const REQUEST_TIMEOUT: i64 = -32105;
}

/// Map an anyhow error to a JSON-RPC error
pub fn map_error(error: AnyhowError) -> JsonRpcError {
    // TODO: Add more specific error mapping based on error type
    // We could use error.downcast_ref::<SpecificError>() here

    tracing::error!("Application error: {}", error);

    JsonRpcError {
        code: ErrorCode::ServerError(error_codes::APPLICATION_ERROR),
        message: format!("Application error: {}", error),
        data: Some(serde_json::to_value(format!("{:?}", error)).unwrap()),
    }
}

/// Convert an error string to a JSON-RPC error
pub fn string_to_error(message: &str, code: i64) -> JsonRpcError {
    JsonRpcError {
        code: ErrorCode::ServerError(code),
        message: message.to_string(),
        data: None,
    }
}
