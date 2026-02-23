/// Simple HTTP response builder for OAuth callback server.
///
/// Provides methods to build HTTP/1.1 responses without requiring heavy
/// dependencies.
pub struct HttpResponse {
    status_line: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpResponse {
    /// Creates a new HTTP response with the given status code and phrase.
    pub fn new(status_code: u16, status_text: &str) -> Self {
        Self {
            status_line: format!("HTTP/1.1 {} {}", status_code, status_text),
            headers: vec![("Connection".to_string(), "close".to_string())],
            body: String::new(),
        }
    }

    /// Adds a header to the response.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Sets the response body.
    pub fn body(mut self, content: String) -> Self {
        self.body = content;
        self
    }

    /// Builds the complete HTTP response as a string.
    pub fn build(self) -> String {
        let mut response = self.status_line;
        response.push_str("\r\n");

        for (name, value) in &self.headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }

        response.push_str("\r\n");
        response.push_str(&self.body);
        response
    }

    /// Creates a 302 redirect response.
    pub fn redirect(location: &str) -> String {
        Self::new(302, "Found")
            .header("Location", location)
            .build()
    }

    /// Creates a 200 OK HTML response.
    pub fn ok_html(html: String) -> String {
        Self::new(200, "OK")
            .header("Content-Type", "text/html; charset=utf-8")
            .body(html)
            .build()
    }

    /// Creates a 400 Bad Request HTML response.
    pub fn bad_request_html(html: String) -> String {
        Self::new(400, "Bad Request")
            .header("Content-Type", "text/html; charset=utf-8")
            .body(html)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_redirect_response() {
        let actual = HttpResponse::redirect("https://example.com/success");
        let expected = "HTTP/1.1 302 Found\r\n\
                       Connection: close\r\n\
                       Location: https://example.com/success\r\n\
                       \r\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_ok_html_response() {
        let actual = HttpResponse::ok_html("<html>Success</html>".to_string());
        let expected = "HTTP/1.1 200 OK\r\n\
                       Connection: close\r\n\
                       Content-Type: text/html; charset=utf-8\r\n\
                       \r\n\
                       <html>Success</html>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bad_request_html_response() {
        let actual = HttpResponse::bad_request_html("<html>Error</html>".to_string());
        let expected = "HTTP/1.1 400 Bad Request\r\n\
                       Connection: close\r\n\
                       Content-Type: text/html; charset=utf-8\r\n\
                       \r\n\
                       <html>Error</html>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_custom_headers() {
        let actual = HttpResponse::new(200, "OK")
            .header("X-Custom", "value")
            .header("X-Another", "test")
            .body("content".to_string())
            .build();

        assert!(actual.contains("HTTP/1.1 200 OK"));
        assert!(actual.contains("X-Custom: value"));
        assert!(actual.contains("X-Another: test"));
        assert!(actual.ends_with("content"));
    }
}
