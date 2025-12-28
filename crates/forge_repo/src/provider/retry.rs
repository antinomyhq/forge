use async_openai::error::{OpenAIError as AsyncOpenAIError, StreamError as AsyncStreamError};
use forge_app::domain::{Error as DomainError, RetryConfig};
use forge_app::dto::openai::{Error, ErrorResponse};

const TRANSPORT_ERROR_CODES: [&str; 3] = ["ERR_STREAM_PREMATURE_CLOSE", "ECONNRESET", "ETIMEDOUT"];

pub fn into_retry(error: anyhow::Error, retry_config: &RetryConfig) -> anyhow::Error {
    if let Some(code) = get_req_status_code(&error)
        .or(get_event_req_status_code(&error))
        .or(get_api_status_code(&error))
        .or(get_async_openai_status_code(&error))
        && retry_config.retry_status_codes.contains(&code)
    {
        return DomainError::Retryable(error).into();
    }

    if is_api_transport_error(&error)
        || is_req_transport_error(&error)
        || is_event_transport_error(&error)
        || is_async_openai_transport_error(&error)
        || is_empty_error(&error)
    {
        return DomainError::Retryable(error).into();
    }

    error
}

fn get_async_openai_status_code(error: &anyhow::Error) -> Option<u16> {
    error
        .downcast_ref::<AsyncOpenAIError>()
        .and_then(|error| match error {
            AsyncOpenAIError::Reqwest(err) => err.status().map(|status| status.as_u16()),
            AsyncOpenAIError::StreamError(err) => match err.as_ref() {
                AsyncStreamError::ReqwestEventSource(inner) => match inner {
                    reqwest_eventsource::Error::InvalidStatusCode(status, _) => {
                        Some(status.as_u16())
                    }
                    reqwest_eventsource::Error::InvalidContentType(_, response) => {
                        Some(response.status().as_u16())
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
}

fn is_async_openai_transport_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<AsyncOpenAIError>()
        .is_some_and(|error| match error {
            AsyncOpenAIError::Reqwest(err) => err.is_timeout() || err.is_connect(),
            AsyncOpenAIError::StreamError(err) => match err.as_ref() {
                AsyncStreamError::ReqwestEventSource(inner) => {
                    matches!(inner, reqwest_eventsource::Error::Transport(_))
                }
                _ => false,
            },
            AsyncOpenAIError::ApiError(api_error) => {
                api_error.code.as_deref().is_some_and(|code| {
                    TRANSPORT_ERROR_CODES.iter().any(|message| message == &code)
                        || matches!(
                            code,
                            "rate_limit_exceeded" | "server_error" | "timeout" | "overloaded"
                        )
                })
            }
            _ => false,
        })
}

fn get_api_status_code(error: &anyhow::Error) -> Option<u16> {
    error.downcast_ref::<Error>().and_then(|error| match error {
        Error::Response(error) => error
            .get_code_deep()
            .as_ref()
            .and_then(|code| code.as_number()),
        Error::InvalidStatusCode(code) => Some(*code),
    })
}

fn get_req_status_code(error: &anyhow::Error) -> Option<u16> {
    error
        .downcast_ref::<reqwest::Error>()
        .and_then(|error| error.status())
        .map(|status| status.as_u16())
}

fn get_event_req_status_code(error: &anyhow::Error) -> Option<u16> {
    error
        .downcast_ref::<reqwest_eventsource::Error>()
        .and_then(|error| match error {
            reqwest_eventsource::Error::InvalidStatusCode(_, response) => {
                Some(response.status().as_u16())
            }
            reqwest_eventsource::Error::InvalidContentType(_, response) => {
                Some(response.status().as_u16())
            }
            _ => None,
        })
}

fn has_transport_error_code(error: &ErrorResponse) -> bool {
    // Check if the current level has a transport error code
    let has_direct_code = error
        .code
        .as_ref()
        .and_then(|code| code.as_str())
        .is_some_and(|code| {
            TRANSPORT_ERROR_CODES
                .into_iter()
                .any(|message| message == code)
        });

    if has_direct_code {
        return true;
    }

    // Recursively check nested errors
    error.error.as_deref().is_some_and(has_transport_error_code)
}

fn is_api_transport_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Error>()
        .is_some_and(|error| match error {
            Error::Response(error) => has_transport_error_code(error),
            _ => false,
        })
}

fn is_empty_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Error>().is_some_and(|e| match e {
        Error::Response(error) => {
            error.message.is_none() && error.code.is_none() && error.error.is_none()
        }
        _ => false,
    })
}

fn is_req_transport_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<reqwest::Error>()
        .is_some_and(|e| e.is_timeout() || e.is_connect())
}

fn is_event_transport_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<reqwest_eventsource::Error>()
        .is_some_and(|e| matches!(e, reqwest_eventsource::Error::Transport(_)))
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use async_openai::error::{ApiError, OpenAIError as AsyncOpenAIError};
    use forge_app::dto::openai::{Error, ErrorCode, ErrorResponse};

    use super::*;

    // Helper function to check if an error is retryable
    fn is_retryable(error: anyhow::Error) -> bool {
        if let Some(domain_error) = error.downcast_ref::<DomainError>() {
            matches!(domain_error, DomainError::Retryable(_))
        } else {
            false
        }
    }

    #[test]
    fn test_into_retry_with_matching_api_status_code() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 503]);
        let inner_error = ErrorResponse::default().code(ErrorCode::Number(500));
        let error = anyhow::Error::from(Error::Response(inner_error));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_non_matching_api_status_code() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 503]);
        let inner_error = ErrorResponse::default().code(ErrorCode::Number(400));
        let error = anyhow::Error::from(Error::Response(inner_error));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify - should not be retryable
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_reqwest_errors() {
        // We can't easily create specific reqwest::Error instances with status codes
        // since they're produced by the HTTP client internally
        // Instead, we'll focus on testing the helper function get_req_status_code

        // Testing the get_req_status_code function directly would be difficult without
        // mocking, and creating a real reqwest::Error with status is not
        // straightforward in tests. In a real-world scenario, this would be
        // tested with integration tests or by mocking the reqwest::Error
        // structure.

        // Verify our function can handle generic errors safely
        let generic_error = anyhow!("A generic error that doesn't have status code");
        let retry_config = RetryConfig::default().retry_status_codes(vec![]);
        let actual = into_retry(generic_error, &retry_config);
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_api_transport_error() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![]);
        let inner_error = ErrorResponse::default()
            .code(ErrorCode::String("ERR_STREAM_PREMATURE_CLOSE".to_string()));
        let error = anyhow::Error::from(Error::Response(inner_error));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify
        assert!(is_retryable(actual));
    }

    // Note: Testing with real reqwest::Error and reqwest_eventsource::Error
    // instances is challenging in unit tests as they're designed to be created
    // internally by their respective libraries during real HTTP operations.
    //
    // For comprehensive testing of these error paths, integration tests would be
    // more appropriate, where actual HTTP requests can be made and real error
    // instances generated.
    //
    // The helper functions (get_req_status_code, get_event_req_status_code, etc.)
    // would ideally be tested with properly mocked errors using a mocking
    // framework.

    #[test]
    fn test_into_retry_with_deep_nested_api_status_code() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 503]);

        // Create deeply nested error with a retryable status code
        let deepest_error = ErrorResponse::default().code(ErrorCode::Number(503));

        let middle_error = ErrorResponse::default().error(Box::new(deepest_error));

        let top_error = ErrorResponse::default().error(Box::new(middle_error));

        let error = anyhow::Error::from(Error::Response(top_error));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_string_error_code_as_number() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 503]);
        let inner_error = ErrorResponse::default().code(ErrorCode::String("429".to_string()));
        let error = anyhow::Error::from(Error::Response(inner_error));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify - should be retryable as "429" can be parsed as a number that matches
        // retry codes
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_non_retryable_error() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![]);
        let generic_error = anyhow!("A generic error that doesn't match any retryable pattern");

        // Execute
        let actual = into_retry(generic_error, &retry_config);

        // Verify
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_invalid_status_code_error() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 503]);
        let error = anyhow::Error::from(Error::InvalidStatusCode(503));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_invalid_status_code_error_non_matching() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 503]);
        let error = anyhow::Error::from(Error::InvalidStatusCode(400));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify - should not be retryable as 400 is not in retry_codes
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_nested_api_transport_error() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![]);
        // Create nested error with transport error code in error.error.code
        let nested_error =
            ErrorResponse::default().code(ErrorCode::String("ECONNRESET".to_string()));

        let top_error = ErrorResponse::default().error(Box::new(nested_error));

        let error = anyhow::Error::from(Error::Response(top_error));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify - should be retryable because ECONNRESET is a transport error
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_deeply_nested_api_transport_error() {
        // Setup
        let retry_config = RetryConfig::default().retry_status_codes(vec![]);
        // Create deeply nested error with transport error code at level 4
        let deepest_error =
            ErrorResponse::default().code(ErrorCode::String("ETIMEDOUT".to_string()));

        let level3_error = ErrorResponse::default().error(Box::new(deepest_error));

        let level2_error = ErrorResponse::default().error(Box::new(level3_error));

        let top_error = ErrorResponse::default().error(Box::new(level2_error));

        let error = anyhow::Error::from(Error::Response(top_error));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify - should be retryable because ETIMEDOUT is a transport error found at
        // level 4
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_is_empty_error_with_default_error_response() {
        // Setup
        let fixture = anyhow::Error::from(Error::Response(ErrorResponse::default()));

        // Execute
        let actual = is_empty_error(&fixture);

        // Verify
        assert!(actual);
    }

    #[test]
    fn test_is_empty_error_with_partially_empty_error_response() {
        // Setup
        let fixture = anyhow::Error::from(Error::Response(ErrorResponse {
            message: None,
            error: None,
            code: None,

            errno: Some(0),
            metadata: vec![("Blah".to_string(), serde_json::Value::Null)]
                .into_iter()
                .collect(),
            syscall: Some("test_syscall".to_string()),
            type_of: Some(serde_json::Value::Null),
            param: Some(serde_json::Value::Null),
        }));

        // Execute
        let actual = is_empty_error(&fixture);
        assert!(actual);
    }

    #[test]
    fn test_is_empty_error_with_message_populated() {
        // Setup
        let fixture = anyhow::Error::from(Error::Response(
            ErrorResponse::default().message("Some error message".to_string()),
        ));

        // Execute
        let actual = is_empty_error(&fixture);

        // Verify
        assert!(!actual);
    }

    #[test]
    fn test_is_empty_error_with_code_populated() {
        // Setup
        let fixture = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(500)),
        ));

        // Execute
        let actual = is_empty_error(&fixture);

        // Verify
        assert!(!actual);
    }

    #[test]
    fn test_is_empty_error_with_nested_error_populated() {
        // Setup
        let nested_error = ErrorResponse::default().message("Nested error".to_string());
        let fixture = anyhow::Error::from(Error::Response(
            ErrorResponse::default().error(Box::new(nested_error)),
        ));

        // Execute
        let actual = is_empty_error(&fixture);

        // Verify
        assert!(!actual);
    }

    #[test]
    fn test_is_empty_error_with_non_response_error() {
        // Setup
        let fixture = anyhow::Error::from(Error::InvalidStatusCode(404));

        // Execute
        let actual = is_empty_error(&fixture);

        // Verify
        assert!(!actual);
    }
    #[test]
    fn test_into_retry_with_async_openai_rate_limit_code_is_retryable() {
        let retry_config = RetryConfig::default().retry_status_codes(vec![]);
        let api_error = ApiError {
            message: "Rate limit".to_string(),
            r#type: Some("rate_limit_error".to_string()),
            param: None,
            code: Some("rate_limit_exceeded".to_string()),
        };

        let error = anyhow::Error::from(AsyncOpenAIError::ApiError(api_error));
        let actual = into_retry(error, &retry_config);
        assert!(is_retryable(actual));
    }

    // Common fixture functions
    fn fixture_retry_config(codes: Vec<u16>) -> RetryConfig {
        RetryConfig::default().retry_status_codes(codes)
    }

    fn fixture_api_error(code: &str) -> ApiError {
        ApiError {
            message: "Test error".to_string(),
            r#type: Some("test_error".to_string()),
            param: None,
            code: Some(code.to_string()),
        }
    }

    #[test]
    fn test_into_retry_with_async_openai_retryable_codes() {
        let retry_config = fixture_retry_config(vec![]);
        let retryable_codes = ["server_error", "timeout", "overloaded"];

        for code in retryable_codes {
            let api_error = fixture_api_error(code);
            let error = anyhow::Error::from(AsyncOpenAIError::ApiError(api_error));
            let actual = into_retry(error, &retry_config);
            assert!(is_retryable(actual), "Code {code} should be retryable");
        }
    }

    #[test]
    fn test_into_retry_with_async_openai_unknown_code_is_not_retryable() {
        let retry_config = fixture_retry_config(vec![]);
        let api_error = ApiError {
            message: "Test error".to_string(),
            r#type: Some("test_error".to_string()),
            param: None,
            code: Some("unknown_error".to_string()),
        };

        let error = anyhow::Error::from(AsyncOpenAIError::ApiError(api_error));
        let actual = into_retry(error, &retry_config);
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_async_openai_api_error_no_code_is_not_retryable() {
        let retry_config = fixture_retry_config(vec![]);
        let api_error = ApiError {
            message: "Test error".to_string(),
            r#type: Some("test_error".to_string()),
            param: None,
            code: None,
        };

        let error = anyhow::Error::from(AsyncOpenAIError::ApiError(api_error));
        let actual = into_retry(error, &retry_config);
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_non_async_openai_error_is_not_retryable() {
        let retry_config = fixture_retry_config(vec![]);
        let error = anyhow!("Some other error");

        let actual = into_retry(error, &retry_config);
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_has_transport_error_code_with_known_codes() {
        let transport_codes = [
            "ERR_STREAM_PREMATURE_CLOSE",
            "ECONNRESET",
            "ETIMEDOUT",
        ];

        for code in transport_codes {
            let error = ErrorResponse::default().code(ErrorCode::String(code.to_string()));
            assert!(has_transport_error_code(&error), "Code {code} should be transport error");
        }
    }

    #[test]
    fn test_has_transport_error_code_with_unknown_code() {
        let error = ErrorResponse::default().code(ErrorCode::String("UNKNOWN_ERROR".to_string()));
        assert!(!has_transport_error_code(&error));
    }

    #[test]
    fn test_has_transport_error_code_with_no_code() {
        let error = ErrorResponse::default();
        assert!(!has_transport_error_code(&error));
    }

    #[test]
    fn test_has_transport_error_code_with_nested_errors() {
        // Test transport code at bottom level
        let level3 = ErrorResponse::default().code(ErrorCode::String("ECONNRESET".to_string()));
        let level2 = ErrorResponse::default().error(Box::new(level3));
        let level1 = ErrorResponse::default().error(Box::new(level2));
        assert!(has_transport_error_code(&level1));

        // Test transport code at top level
        let level2 = ErrorResponse::default().code(ErrorCode::String("UNKNOWN".to_string()));
        let level1 = ErrorResponse::default()
            .code(ErrorCode::String("ERR_STREAM_PREMATURE_CLOSE".to_string()))
            .error(Box::new(level2));
        assert!(has_transport_error_code(&level1));

        // Test 3-level nesting with transport at bottom
        let level3 = ErrorResponse::default().code(ErrorCode::String("ETIMEDOUT".to_string()));
        let level2 = ErrorResponse::default().error(Box::new(level3));
        let level1 = ErrorResponse::default().error(Box::new(level2));
        assert!(has_transport_error_code(&level1));
    }

    #[test]
    fn test_has_transport_error_code_with_nested_unknown_code() {
        let nested = ErrorResponse::default().code(ErrorCode::String("UNKNOWN".to_string()));
        let error = ErrorResponse::default().error(Box::new(nested));
        assert!(!has_transport_error_code(&error));
    }

    #[test]
    fn test_is_api_transport_error_with_transport_code() {
        let inner_error = ErrorResponse::default().code(ErrorCode::String("ETIMEDOUT".to_string()));
        let error = anyhow::Error::from(Error::Response(inner_error));

        let actual = is_api_transport_error(&error);
        assert!(actual);
    }

    #[test]
    fn test_is_api_transport_error_with_non_transport_code() {
        let inner_error =
            ErrorResponse::default().code(ErrorCode::String("INVALID_REQUEST".to_string()));
        let error = anyhow::Error::from(Error::Response(inner_error));

        let actual = is_api_transport_error(&error);
        assert!(!actual);
    }

    #[test]
    fn test_is_api_transport_error_with_invalid_status_code() {
        let error = anyhow::Error::from(Error::InvalidStatusCode(500));

        let actual = is_api_transport_error(&error);
        assert!(!actual);
    }

    #[test]
    fn test_generic_error_handlers_return_defaults() {
        let error = anyhow!("Generic error");

        // All transport error handlers return false for generic errors
        assert!(!is_api_transport_error(&error));
        assert!(!is_req_transport_error(&error));
        assert!(!is_event_transport_error(&error));
        assert!(!is_async_openai_transport_error(&error));

        // All status code getters return None for generic errors
        assert!(get_async_openai_status_code(&error).is_none());
        assert!(get_api_status_code(&error).is_none());
        assert!(get_req_status_code(&error).is_none());
        assert!(get_event_req_status_code(&error).is_none());
    }

    #[test]
    fn test_into_retry_with_empty_retry_config() {
        let retry_config = RetryConfig::default().retry_status_codes(vec![]);
        let inner_error = ErrorResponse::default().code(ErrorCode::Number(500));
        let error = anyhow::Error::from(Error::Response(inner_error));

        let actual = into_retry(error, &retry_config);

        // Should not be retryable since 500 is not in empty retry_codes
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_all_retryable_status_codes() {
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 502, 503, 504]);

        // Test 429
        let error_429 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(429)),
        ));
        assert!(is_retryable(into_retry(error_429, &retry_config)));

        // Test 500
        let error_500 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(500)),
        ));
        assert!(is_retryable(into_retry(error_500, &retry_config)));

        // Test 502
        let error_502 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(502)),
        ));
        assert!(is_retryable(into_retry(error_502, &retry_config)));

        // Test 503
        let error_503 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(503)),
        ));
        assert!(is_retryable(into_retry(error_503, &retry_config)));

        // Test 504
        let error_504 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(504)),
        ));
        assert!(is_retryable(into_retry(error_504, &retry_config)));
    }

    #[test]
    fn test_into_retry_with_non_retryable_status_codes() {
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 502, 503, 504]);

        // Test 400 - not in retryable list
        let error_400 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(400)),
        ));
        assert!(!is_retryable(into_retry(error_400, &retry_config)));

        // Test 401 - not in retryable list
        let error_401 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(401)),
        ));
        assert!(!is_retryable(into_retry(error_401, &retry_config)));

        // Test 403 - not in retryable list
        let error_403 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(403)),
        ));
        assert!(!is_retryable(into_retry(error_403, &retry_config)));

        // Test 404 - not in retryable list
        let error_404 = anyhow::Error::from(Error::Response(
            ErrorResponse::default().code(ErrorCode::Number(404)),
        ));
        assert!(!is_retryable(into_retry(error_404, &retry_config)));
    }

    #[test]
    fn test_into_retry_with_empty_error_and_empty_retry_config() {
        let retry_config = RetryConfig::default().retry_status_codes(vec![]);
        let error = anyhow::Error::from(Error::Response(ErrorResponse::default()));

        // Empty error should be retryable regardless of retry config
        let actual = into_retry(error, &retry_config);
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_string_status_code_not_in_retry_codes() {
        let retry_config = RetryConfig::default().retry_status_codes(vec![429, 500, 503]);
        let inner_error = ErrorResponse::default().code(ErrorCode::String("404".to_string()));
        let error = anyhow::Error::from(Error::Response(inner_error));

        let actual = into_retry(error, &retry_config);

        // Should not be retryable as 404 is not in retry_codes
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_has_transport_error_code_with_no_transport_anywhere() {
        // Create a deeply nested error with no transport codes
        let level3 = ErrorResponse::default().code(ErrorCode::String("UNKNOWN".to_string()));
        let level2 = ErrorResponse::default().error(Box::new(level3));
        let level1 = ErrorResponse::default().error(Box::new(level2));

        assert!(!has_transport_error_code(&level1));
    }

    #[test]
    fn test_is_empty_error_with_only_nested_error() {
        // Empty at top level, but nested error has content
        let nested = ErrorResponse::default().message("Nested error".to_string());
        let fixture = anyhow::Error::from(Error::Response(
            ErrorResponse::default().error(Box::new(nested)),
        ));

        let actual = is_empty_error(&fixture);
        assert!(!actual);
    }

    #[test]
    fn test_is_empty_error_with_nested_code_only() {
        // Empty at top level, but nested error has code
        let nested = ErrorResponse::default().code(ErrorCode::Number(500));
        let fixture = anyhow::Error::from(Error::Response(
            ErrorResponse::default().error(Box::new(nested)),
        ));

        let actual = is_empty_error(&fixture);
        assert!(!actual);
    }

    #[test]
    fn test_is_empty_error_with_all_fields_populated() {
        let fixture = anyhow::Error::from(Error::Response(ErrorResponse {
            message: Some("Error message".to_string()),
            code: Some(ErrorCode::Number(500)),
            error: Some(Box::new(ErrorResponse::default())),
            errno: None,
            metadata: std::collections::BTreeMap::new(),
            syscall: None,
            type_of: None,
            param: None,
        }));

        let actual = is_empty_error(&fixture);
        assert!(!actual);
    }
}
