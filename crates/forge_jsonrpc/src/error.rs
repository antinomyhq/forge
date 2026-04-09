use jsonrpsee::types::{ErrorObject, ErrorObjectOwned};
use serde::Serialize;

/// JSON-RPC error codes
pub struct ErrorCode;

impl ErrorCode {
    /// Parse error (-32700)
    pub const PARSE_ERROR: i32 = -32700;
    /// Invalid request (-32600)
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found (-32601)
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid params (-32602)
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error (-32603)
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Server error base (-32000)
    pub const SERVER_ERROR: i32 = -32000;
    /// Not found (-32001)
    pub const NOT_FOUND: i32 = -32001;
    /// Unauthorized (-32002)
    pub const UNAUTHORIZED: i32 = -32002;
    /// Validation failed (-32003)
    pub const VALIDATION_FAILED: i32 = -32003;
}

/// Convert anyhow errors to JSON-RPC errors using proper downcasting
pub fn map_error(err: anyhow::Error) -> ErrorObjectOwned {
    // Try to downcast to specific domain error types
    if let Some(domain_err) = err.downcast_ref::<forge_domain::Error>() {
        return map_domain_error(domain_err);
    }

    // Try to downcast to app error types
    if let Some(app_err) = err.downcast_ref::<forge_app::Error>() {
        return map_app_error(app_err);
    }

    // Default: return as internal error without string matching
    ErrorObject::owned(
        ErrorCode::INTERNAL_ERROR,
        format!("Internal error: {}", err),
        None::<()>,
    )
}

/// Map domain errors to JSON-RPC errors
fn map_domain_error(err: &forge_domain::Error) -> ErrorObjectOwned {
    match err {
        forge_domain::Error::ConversationNotFound(_) |
        forge_domain::Error::AgentUndefined(_) |
        forge_domain::Error::WorkspaceNotFound |
        forge_domain::Error::HeadAgentUndefined => {
            ErrorObject::owned(ErrorCode::NOT_FOUND, err.to_string(), None::<()>)
        }
        forge_domain::Error::ProviderNotAvailable { .. } |
        forge_domain::Error::EnvironmentVariableNotFound { .. } |
        forge_domain::Error::AuthTokenNotFound => {
            ErrorObject::owned(ErrorCode::UNAUTHORIZED, err.to_string(), None::<()>)
        }
        forge_domain::Error::ConversationId(_) |
        forge_domain::Error::ToolCallArgument { .. } |
        forge_domain::Error::AgentCallArgument { .. } |
        forge_domain::Error::ToolCallParse(_) |
        forge_domain::Error::ToolCallMissingName |
        forge_domain::Error::ToolCallMissingId |
        forge_domain::Error::EToolCallArgument(_) |
        forge_domain::Error::MissingAgentDescription(_) |
        forge_domain::Error::MissingModel(_) |
        forge_domain::Error::NoModelDefined(_) |
        forge_domain::Error::NoDefaultSession => {
            ErrorObject::owned(ErrorCode::VALIDATION_FAILED, err.to_string(), None::<()>)
        }
        forge_domain::Error::MaxTurnsReached(_, _) |
        forge_domain::Error::WorkspaceAlreadyInitialized(_) |
        forge_domain::Error::SyncFailed { .. } |
        forge_domain::Error::EmptyCompletion |
        forge_domain::Error::VertexAiConfiguration { .. } |
        forge_domain::Error::Retryable(_) |
        forge_domain::Error::UnsupportedRole(_) |
        forge_domain::Error::UndefinedVariable(_) => {
            ErrorObject::owned(ErrorCode::INTERNAL_ERROR, err.to_string(), None::<()>)
        }
    }
}

/// Map app errors to JSON-RPC errors
fn map_app_error(err: &forge_app::Error) -> ErrorObjectOwned {
    use forge_app::Error;
    match err {
        Error::NotFound(_) => ErrorObject::owned(ErrorCode::NOT_FOUND, err.to_string(), None::<()>),
        Error::CallArgument(_) | Error::CallTimeout { .. } => {
            ErrorObject::owned(ErrorCode::VALIDATION_FAILED, err.to_string(), None::<()>)
        }
        _ => ErrorObject::owned(ErrorCode::INTERNAL_ERROR, err.to_string(), None::<()>),
    }
}

/// Create a method not found error
pub fn method_not_found(method: &str) -> ErrorObjectOwned {
    ErrorObject::owned(
        ErrorCode::METHOD_NOT_FOUND,
        format!("Method not found: {}", method),
        Some(serde_json::json!({ "method": method })),
    )
}

/// Create an invalid params error
pub fn invalid_params<T: Serialize>(message: &str, data: T) -> ErrorObjectOwned {
    ErrorObject::owned(
        ErrorCode::INVALID_PARAMS,
        format!("Invalid params: {}", message),
        Some(data),
    )
}

/// Create a not found error
pub fn not_found(resource: &str, id: &str) -> ErrorObjectOwned {
    ErrorObject::owned(
        ErrorCode::NOT_FOUND,
        format!("{} not found: {}", resource, id),
        Some(serde_json::json!({ "resource": resource, "id": id })),
    )
}

/// Create an internal error
pub fn internal_error(message: &str) -> ErrorObjectOwned {
    ErrorObject::owned(
        ErrorCode::INTERNAL_ERROR,
        format!("Internal error: {}", message),
        None::<()>,
    )
}
