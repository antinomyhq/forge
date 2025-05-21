use forge_domain::Error as DomainError;

use crate::error::Error;
pub fn into_retry(error: anyhow::Error, retry_status_codes: &[u16]) -> anyhow::Error {
    if let Some(code) = get_req_status_code(&error)
        .or(get_event_req_status_code(&error))
        .or(get_api_status_code(&error))
    {
        if retry_status_codes.contains(&code) {
            return DomainError::Retryable(error).into();
        }
    }

    if is_api_transport_error(&error)
        || is_req_transport_error(&error)
        || is_event_transport_error(&error)
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
        _ => None,
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

fn is_api_transport_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Error>()
        .is_some_and(|error| match error {
            Error::Response(error) => error
                .code
                .as_ref()
                .and_then(|code| code.as_str())
                .is_some_and(|code| {
                    ["ERR_STREAM_PREMATURE_CLOSE", "ECONNRESET", "ETIMEDOUT"]
                        .into_iter()
                        .any(|message| message == code)
                }),
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

    use super::*;
    use crate::error::{Error, ErrorCode, ErrorResponse};

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
        let retry_codes = vec![429, 500, 503];
        let inner_error = ErrorResponse::default().code(Some(ErrorCode::Number(500)));
        let error = anyhow::Error::from(Error::Response(inner_error));

        // Execute
        let actual = into_retry(error, &retry_codes);

        // Verify
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_non_matching_api_status_code() {
        // Setup
        let retry_codes = vec![429, 500, 503];
        let inner_error = ErrorResponse::default().code(Some(ErrorCode::Number(400)));
        let error = anyhow::Error::from(Error::Response(inner_error));

        // Execute
        let actual = into_retry(error, &retry_codes);

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
        let retry_codes = vec![429, 500, 503];
        let generic_error = anyhow!("A generic error that doesn't have status code");
        let actual = into_retry(generic_error, &retry_codes);
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_api_transport_error() {
        // Setup
        let retry_codes = vec![429, 500, 503];
        let inner_error = ErrorResponse::default().code(Some(ErrorCode::String(
            "ERR_STREAM_PREMATURE_CLOSE".to_string(),
        )));
        let error = anyhow::Error::from(Error::Response(inner_error));

        // Execute
        let actual = into_retry(error, &retry_codes);

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
        let retry_codes = vec![429, 500, 503];

        // Create deeply nested error with a retryable status code
        let deepest_error = ErrorResponse::default().code(Some(ErrorCode::Number(503)));

        let middle_error = ErrorResponse::default().error(Some(Box::new(deepest_error)));

        let top_error = ErrorResponse::default().error(Some(Box::new(middle_error)));

        let error = anyhow::Error::from(Error::Response(top_error));

        // Execute
        let actual = into_retry(error, &retry_codes);

        // Verify
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_string_error_code_as_number() {
        // Setup
        let retry_codes = vec![429, 500, 503];
        let inner_error = ErrorResponse::default().code(Some(ErrorCode::String("429".to_string())));
        let error = anyhow::Error::from(Error::Response(inner_error));

        // Execute
        let actual = into_retry(error, &retry_codes);

        // Verify - should be retryable as "429" can be parsed as a number that matches
        // retry codes
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_non_retryable_error() {
        // Setup
        let retry_codes = vec![429, 500, 503];
        let generic_error = anyhow!("A generic error that doesn't match any retryable pattern");

        // Execute
        let actual = into_retry(generic_error, &retry_codes);

        // Verify
        assert!(!is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_invalid_status_code_error() {
        // Setup
        let retry_codes = vec![429, 500, 503];
        let error = anyhow::Error::from(Error::InvalidStatusCode(503));

        // Execute
        let actual = into_retry(error, &retry_codes);

        // Verify
        assert!(is_retryable(actual));
    }

    #[test]
    fn test_into_retry_with_invalid_status_code_error_non_matching() {
        // Setup
        let retry_codes = vec![429, 500, 503];
        let error = anyhow::Error::from(Error::InvalidStatusCode(400));

        // Execute
        let actual = into_retry(error, &retry_codes);

        // Verify - should not be retryable as 400 is not in retry_codes
        assert!(!is_retryable(actual));
    }
}
