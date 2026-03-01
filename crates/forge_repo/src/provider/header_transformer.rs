use forge_domain::Transformer;
use reqwest::header::HeaderMap;

/// Transformer for adding OpenCode Zen headers
pub struct OpenCodeZenHeaders {
    project_id: String,
    session_id: Option<String>,
    user_id: String,
}

impl OpenCodeZenHeaders {
    pub fn new(session_id: Option<String>) -> Self {
        // x-opencode-project: Use workspace directory name as project identifier
        let project_id = std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "unknown".to_string());

        // x-opencode-request: Use system username
        let user_id = whoami::username().unwrap_or_else(|_| "unknown".to_string());

        // x-opencode-session: Use provided session_id or generate a temporary one
        // OpenCode Zen requires this header to avoid strict rate limiting
        let session_id = session_id.or_else(|| {
            use std::time::{SystemTime, UNIX_EPOCH};
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            // Generate temporary session ID for non-conversation commands (e.g., suggest)
            Some(format!("forge-temp-{}", timestamp))
        });

        Self { project_id, session_id, user_id }
    }
}

impl Transformer for OpenCodeZenHeaders {
    type Value = HeaderMap;

    fn transform(&mut self, mut headers: Self::Value) -> Self::Value {
        use reqwest::header::HeaderValue;

        // x-opencode-project
        headers.insert(
            "x-opencode-project",
            HeaderValue::from_str(&self.project_id)
                .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
        );

        // x-opencode-session (always present since we generate if needed)
        if let Some(session_id) = &self.session_id {
            headers.insert(
                "x-opencode-session",
                HeaderValue::from_str(session_id)
                    .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
            );
        }

        // x-opencode-request
        headers.insert(
            "x-opencode-request",
            HeaderValue::from_str(&self.user_id)
                .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
        );

        // x-opencode-client
        headers.insert("x-opencode-client", HeaderValue::from_static("cli"));

        headers
    }
}

/// Transformer for adding Session-Id header for zai providers
pub struct ZaiSessionHeader {
    session_id: String,
}

impl ZaiSessionHeader {
    pub fn new(session_id: String) -> Self {
        Self { session_id }
    }
}

impl Transformer for ZaiSessionHeader {
    type Value = HeaderMap;

    fn transform(&mut self, mut headers: Self::Value) -> Self::Value {
        use reqwest::header::HeaderValue;

        headers.insert(
            "Session-Id",
            HeaderValue::from_str(&self.session_id)
                .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
        );

        headers
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_opencode_zen_headers_transform() {
        let mut transformer = OpenCodeZenHeaders::new(Some("test-session-123".to_string()));
        let headers = HeaderMap::new();

        let actual = transformer.transform(headers);

        // Should have 4 OpenCode headers
        assert!(actual.contains_key("x-opencode-project"));
        assert!(actual.contains_key("x-opencode-session"));
        assert!(actual.contains_key("x-opencode-request"));
        assert!(actual.contains_key("x-opencode-client"));

        // Check values
        assert_eq!(
            actual.get("x-opencode-session").unwrap(),
            "test-session-123"
        );
        assert_eq!(actual.get("x-opencode-client").unwrap(), "cli");
    }

    #[test]
    fn test_opencode_zen_headers_without_session() {
        let mut transformer = OpenCodeZenHeaders::new(None);
        let headers = HeaderMap::new();

        let actual = transformer.transform(headers);

        // Should have 4 OpenCode headers (auto-generated session)
        assert!(actual.contains_key("x-opencode-project"));
        assert!(actual.contains_key("x-opencode-session")); // Now auto-generated
        assert!(actual.contains_key("x-opencode-request"));
        assert!(actual.contains_key("x-opencode-client"));

        // Verify the session was auto-generated with "forge-temp-" prefix
        let session = actual.get("x-opencode-session").unwrap().to_str().unwrap();
        assert!(
            session.starts_with("forge-temp-"),
            "Session should start with 'forge-temp-' but was: {}",
            session
        );
    }

    #[test]
    fn test_zai_session_header_transform() {
        let mut transformer = ZaiSessionHeader::new("zai-session-456".to_string());
        let headers = HeaderMap::new();

        let actual = transformer.transform(headers);

        assert!(actual.contains_key("Session-Id"));
        assert_eq!(actual.get("Session-Id").unwrap(), "zai-session-456");
    }
}
