/// Represents a curl command with type-safe components
#[derive(Debug, Clone, Default, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct CurlCommand {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl CurlCommand {
    /// Creates a new curl command builder
    pub fn new(method: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    /// Renders the curl command as a string
    pub fn render(&self) -> String {
        let mut cmd = format!("curl -X {} '{}'", self.method, self.url);

        for (key, value) in &self.headers {
            cmd.push_str(&format!(" \\\n  -H '{}: {}'", key, value));
        }

        if let Some(body) = &self.body {
            // Escape single quotes in the body
            let escaped_body = body.replace('\'', "'\\''");
            cmd.push_str(&format!(" \\\n  -d '{}'", escaped_body));
        }

        cmd
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::CurlCommand;

    #[test]
    fn test_basic_get() {
        let actual = CurlCommand::new("GET", "https://api.example.com/users").render();
        let expected = "curl -X GET 'https://api.example.com/users'";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_headers() {
        let actual = CurlCommand::new("POST", "https://api.example.com/data")
            .headers(vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Authorization".to_string(), "Bearer token123".to_string()),
            ])
            .render();
        let expected = "curl -X POST 'https://api.example.com/data' \
            \\\n  -H 'Content-Type: application/json' \
            \\\n  -H 'Authorization: Bearer token123'";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_body() {
        let actual = CurlCommand::new("POST", "https://api.example.com/messages")
            .body(r#"{"message":"hello"}"#.to_string())
            .render();
        let expected = "curl -X POST 'https://api.example.com/messages' \
            \\\n  -d '{\"message\":\"hello\"}'";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_body_with_single_quotes_escaped() {
        let actual = CurlCommand::new("POST", "https://api.example.com/data")
            .body("it's working".to_string())
            .render();
        let expected = "curl -X POST 'https://api.example.com/data' \
            \\\n  -d 'it'\\''s working'";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_full_request() {
        let actual = CurlCommand::new("PUT", "https://api.example.com/users/1")
            .headers(vec![("Content-Type".to_string(), "application/json".to_string())])
            .body(r#"{"name":"John"}"#.to_string())
            .render();
        let expected = "curl -X PUT 'https://api.example.com/users/1' \
            \\\n  -H 'Content-Type: application/json' \
            \\\n  -d '{\"name\":\"John\"}'";
        assert_eq!(actual, expected);
    }
}
