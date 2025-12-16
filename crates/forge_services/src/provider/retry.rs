use forge_app::domain::{Error as DomainError, RetryConfig};
use forge_app::dto::openai::{Error, ErrorResponse};

const TRANSPORT_ERROR_CODES: [&str; 3] = ["ERR_STREAM_PREMATURE_CLOSE", "ECONNRESET", "ETIMEDOUT"];

pub fn into_retry(error: anyhow::Error, retry_config: &RetryConfig) -> anyhow::Error {
    if let Some(code) = get_req_status_code(&error)
        .or(get_event_req_status_code(&error))
        .or(get_api_status_code(&error))
        && retry_config.retry_status_codes.contains(&code)
    {
        return DomainError::Retryable(error).into();
    }

    if is_api_transport_error(&error)
        || is_req_transport_error(&error)
        || is_event_transport_error(&error)
        || is_empty_error(&error)
    {
        return DomainError::Retryable(error).into();
    }

    error
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
    use fake::{Fake, Faker};
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
        use fake::{Fake, Faker};

        // Setup
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![429, 500, 503]);
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
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![429, 500, 503]);
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
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![]);
        let actual = into_retry(generic_error, &retry_config);
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_api_transport_error() {
        // Setup
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![]);
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
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![429, 500, 503]);

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
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![429, 500, 503]);
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
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![]);
        let generic_error = anyhow!("A generic error that doesn't match any retryable pattern");

        // Execute
        let actual = into_retry(generic_error, &retry_config);

        // Verify
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_invalid_status_code_error() {
        // Setup
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![429, 500, 503]);
        let error = anyhow::Error::from(Error::InvalidStatusCode(503));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_invalid_status_code_error_non_matching() {
        // Setup
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![429, 500, 503]);
        let error = anyhow::Error::from(Error::InvalidStatusCode(400));

        // Execute
        let actual = into_retry(error, &retry_config);

        // Verify - should not be retryable as 400 is not in retry_codes
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_nested_api_transport_error() {
        // Setup
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![]);
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
        let retry_config: RetryConfig = Faker.fake();
        let retry_config = retry_config.retry_status_codes(vec![]);
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
}
