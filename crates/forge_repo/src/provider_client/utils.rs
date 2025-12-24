use anyhow::Context as _;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::{StatusCode, Url};

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

/// Joins a base URL with a path, validating the path for security
///
/// # Errors
///
/// Returns an error if the path contains forbidden patterns or if URL parsing
/// fails
pub fn join_url(base_url: &str, path: &str) -> anyhow::Result<Url> {
    // Validate the path doesn't contain certain patterns
    if path.contains("://") || path.contains("..") {
        anyhow::bail!("Invalid path: Contains forbidden patterns");
    }

    // Remove leading slash to avoid double slashes
    let path = path.trim_start_matches('/');

    let url = Url::parse(base_url)
        .with_context(|| format!("Failed to parse base URL: {base_url}"))?
        .join(path)
        .with_context(|| format!("Failed to append {path} to base URL: {base_url}"))?;
    Ok(url)
}

/// Creates a HeaderMap from a vector of header key-value pairs
pub fn create_headers(headers: Vec<(String, String)>) -> HeaderMap {
    let mut header_map = HeaderMap::new();
    for (key, value) in headers {
        let header_name =
            reqwest::header::HeaderName::from_bytes(key.as_bytes()).expect("Invalid header name");
        let header_value = value.parse().expect("Invalid header value");
        header_map.insert(header_name, header_value);
    }
    header_map
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
