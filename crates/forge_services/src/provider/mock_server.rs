use mockito::{Mock, Server, ServerGuard};

pub struct MockServer {
    server: ServerGuard,
}

impl MockServer {
    pub async fn new() -> Self {
        let server = Server::new_async().await;
        Self { server }
    }
    pub async fn mock_models(&mut self, body: serde_json::Value, status: usize) -> Mock {
        self.server
            .mock("GET", "/models")
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(body.to_string())
            .create_async()
            .await
    }

    pub async fn mock_responses(&mut self, body: serde_json::Value, status: usize) -> Mock {
        self.server
            .mock("POST", "/v1/responses")
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(body.to_string())
            .create_async()
            .await
    }

    pub fn url(&self) -> String {
        self.server.url()
    }
}

/// Normalize dynamic addresses in messages for testing/logging.
pub fn normalize_ports(input: String) -> String {
    use regex::Regex;

    let re_ip_port = Regex::new(r"127\.0\.0\.1:\d+").unwrap();
    let re_http = Regex::new(r"http://127\.0\.0\.1:\d+").unwrap();

    let normalized = re_http.replace_all(&input, "http://127.0.0.1:<port>");
    let normalized = re_ip_port.replace_all(&normalized, "127.0.0.1:<port>");

    normalized.to_string()
}
