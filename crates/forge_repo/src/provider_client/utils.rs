use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::StatusCode;

/// Helper function to format HTTP request/response context for logging and
/// error reporting
pub(crate) fn format_http_context<U: AsRef<str>>(
    status: Option<StatusCode>,
    method: &str,
    url: U,
) -> String {
    if let Some(status) = status {
        format!("{} {} {}", status.as_u16(), method, url.as_ref())
    } else {
        format!("{} {}", method, url.as_ref())
    }
}

/// Sanitizes headers for logging by redacting sensitive values
pub fn sanitize_headers(headers: &HeaderMap) -> HeaderMap {
    let sensitive_headers = [AUTHORIZATION.as_str()];
    headers
        .iter()
        .map(|(name, value)| {
            let name_str = name.as_str().to_lowercase();
            let value_str = if sensitive_headers.contains(&name_str.as_str()) {
                HeaderValue::from_static("[REDACTED]")
            } else {
                value.clone()
            };
            (name.clone(), value_str)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;

    use super::*;

    #[test]
    fn test_sanitize_headers_for_logging() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-api-key"),
        );
        headers.insert("x-api-key", HeaderValue::from_static("another-secret"));
        headers.insert("x-title", HeaderValue::from_static("forge"));
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let sanitized = sanitize_headers(&headers);

        assert_eq!(
            sanitized.get("authorization"),
            Some(&HeaderValue::from_static("[REDACTED]"))
        );
        assert_eq!(
            sanitized.get("x-title"),
            Some(&HeaderValue::from_static("forge"))
        );
        assert_eq!(
            sanitized.get("content-type"),
            Some(&HeaderValue::from_static("application/json"))
        );
    }
}
