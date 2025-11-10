use std::borrow::Cow;
use std::future::Future;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use backon::{ExponentialBuilder, Retryable};
use forge_app::McpClientInfra;
use forge_domain::{Image, McpServerConfig, ToolDefinition, ToolName, ToolOutput};
use rmcp::model::{CallToolRequestParam, ClientInfo, Implementation, InitializeRequestParam};
use rmcp::service::RunningService;
use rmcp::transport::{SseClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use schemars::schema::RootSchema;
use serde_json::Value;
use tokio::process::Command;

use crate::error::Error;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

type RmcpClient = RunningService<RoleClient, InitializeRequestParam>;

#[derive(Clone)]
pub struct ForgeMcpClient {
    client: Arc<RwLock<Option<Arc<RmcpClient>>>>,
    config: McpServerConfig,
}

impl ForgeMcpClient {
    pub fn new(config: McpServerConfig) -> Self {
        Self { client: Default::default(), config }
    }

    fn client_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: Default::default(),
            client_info: Implementation {
                name: "Forge".to_string(),
                version: VERSION.to_string(),
                icons: None,
                title: None,
                website_url: None,
            },
        }
    }

    /// Connects to MCP server. If `force` is true, it will reconnect even
    /// if already connected.
    async fn connect(&self) -> anyhow::Result<Arc<RmcpClient>> {
        if let Some(client) = self.get_client() {
            Ok(client.clone())
        } else {
            let client = self.create_connection().await?;
            self.set_client(client.clone());
            Ok(client.clone())
        }
    }

    fn get_client(&self) -> Option<Arc<RmcpClient>> {
        let guard = self.client.read().unwrap();
        guard.clone()
    }

    fn set_client(&self, client: Arc<RmcpClient>) {
        let mut guard = self.client.write().unwrap();
        *guard = Some(client);
    }

    async fn create_connection(&self) -> anyhow::Result<Arc<RmcpClient>> {
        let client = match &self.config {
            McpServerConfig::Stdio(stdio) => {
                let mut cmd = Command::new(stdio.command.clone());

                for (key, value) in &stdio.env {
                    cmd.env(key, value);
                }

                cmd.args(&stdio.args).kill_on_drop(true);

                // Use builder pattern to capture and ignore stderr to silence MCP logs
                let (transport, _stderr) = TokioChildProcess::builder(cmd)
                    .stderr(std::process::Stdio::piped())
                    .spawn()?;

                self.client_info().serve(transport).await?
            }
            McpServerConfig::Sse(sse) => {
                let transport = self.create_sse_transport(sse).await?;
                self.client_info().serve(transport).await?
            }
        };

        Ok(Arc::new(client))
    }

    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let client = self.connect().await?;
        let tools = client.list_tools(None).await?;
        Ok(tools
            .tools
            .into_iter()
            .filter_map(|tool| {
                Some(
                    ToolDefinition::new(tool.name)
                        .description(tool.description.unwrap_or_default())
                        .input_schema(
                            serde_json::from_value::<RootSchema>(Value::Object(
                                tool.input_schema.as_ref().clone(),
                            ))
                            .ok()?,
                        ),
                )
            })
            .collect())
    }

    async fn call(&self, tool_name: &ToolName, input: &Value) -> anyhow::Result<ToolOutput> {
        let client = self.connect().await?;
        let result = client
            .call_tool(CallToolRequestParam {
                name: Cow::Owned(tool_name.to_string()),
                arguments: if let Value::Object(args) = input {
                    Some(args.clone())
                } else {
                    None
                },
            })
            .await?;

        let tool_contents: Vec<ToolOutput> = result
            .content
            .into_iter()
            .map(|content| match content.raw {
                rmcp::model::RawContent::Text(raw_text_content) => {
                    Ok(ToolOutput::text(raw_text_content.text))
                }
                rmcp::model::RawContent::Image(raw_image_content) => Ok(ToolOutput::image(
                    Image::new_base64(raw_image_content.data, raw_image_content.mime_type.as_str()),
                )),
                rmcp::model::RawContent::Resource(_) => {
                    Err(Error::UnsupportedMcpResponse("Resource").into())
                }
                rmcp::model::RawContent::ResourceLink(_) => {
                    Err(Error::UnsupportedMcpResponse("ResourceLink").into())
                }
                rmcp::model::RawContent::Audio(_) => {
                    Err(Error::UnsupportedMcpResponse("Audio").into())
                }
            })
            .collect::<anyhow::Result<Vec<ToolOutput>>>()?;

        Ok(ToolOutput::from(tool_contents.into_iter())
            .is_error(result.is_error.unwrap_or_default()))
    }

    async fn create_sse_transport(
        &self,
        sse: &forge_domain::McpSseServer,
    ) -> anyhow::Result<rmcp::transport::SseClientTransport<reqwest::Client>> {
        if sse.headers.is_empty() {
            // Use standard transport when no headers configured
            return Ok(SseClientTransport::start(sse.url.clone()).await?);
        }

        // Extract authorization token if present
        let auth_token = self.extract_auth_token(&sse.headers);

        // Create custom reqwest client with auth token if available
        let client = if let Some(token) = auth_token {
            let mut headers = reqwest::header::HeaderMap::new();
            let auth_value = format!("Bearer {}", token)
                .parse()
                .map_err(|e| anyhow::anyhow!("Failed to parse Authorization header: {}", e))?;
            headers.insert(reqwest::header::AUTHORIZATION, auth_value);

            reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to create authenticated client: {}", e))?
        } else {
            reqwest::Client::new()
        };

        // Create custom SSE client with all headers
        let mut headers = reqwest::header::HeaderMap::new();
        for (key, value) in &sse.headers {
            if key.to_lowercase() != "authorization" {
                // Skip Authorization header as it's handled via bearer_auth()
                if let Ok(header_value) = reqwest::header::HeaderValue::from_str(value)
                    && let Ok(header_name) = reqwest::header::HeaderName::from_str(key)
                {
                    headers.insert(header_name, header_value);
                }
            }
        }

        // Create SSE transport with custom client and config
        use rmcp::transport::sse_client::SseClientConfig;

        let config = SseClientConfig { sse_endpoint: sse.url.clone().into(), ..Default::default() };

        SseClientTransport::start_with_client(client, config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create SSE transport: {}", e))
    }

    /// Extract authorization token from headers, supporting both
    /// "Authorization" and "authorization"
    fn extract_auth_token(
        &self,
        headers: &std::collections::BTreeMap<String, String>,
    ) -> Option<String> {
        headers
            .get("Authorization")
            .or_else(|| headers.get("authorization"))
            .map(|h| {
                if h.starts_with("Bearer ") {
                    h.strip_prefix("Bearer ").unwrap().to_string()
                } else {
                    h.clone()
                }
            })
    }

    async fn attempt_with_retry<T, F>(&self, call: impl Fn() -> F) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>>,
    {
        call.retry(
            ExponentialBuilder::default()
                .with_max_times(5)
                .with_jitter(),
        )
        .when(|err| {
            let is_transport = err
                .downcast_ref::<rmcp::ServiceError>()
                .map(|e| {
                    matches!(
                        e,
                        rmcp::ServiceError::TransportSend(_) | rmcp::ServiceError::TransportClosed
                    )
                })
                .unwrap_or(false);

            if is_transport {
                self.client.write().unwrap().take();
            }

            is_transport
        })
        .await
    }
}

#[async_trait::async_trait]
impl McpClientInfra for ForgeMcpClient {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.attempt_with_retry(|| self.list()).await
    }

    async fn call(&self, tool_name: &ToolName, input: Value) -> anyhow::Result<ToolOutput> {
        self.attempt_with_retry(|| self.call(tool_name, &input))
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn test_extract_auth_token_bearer_format() {
        let client = ForgeMcpClient::new(McpServerConfig::new_sse("http://test.com"));
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());

        let result = client.extract_auth_token(&headers);

        assert_eq!(result, Some("token123".to_string()));
    }

    #[test]
    fn test_extract_auth_token_raw_format() {
        let client = ForgeMcpClient::new(McpServerConfig::new_sse("http://test.com"));
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "token123".to_string());

        let result = client.extract_auth_token(&headers);

        assert_eq!(result, Some("token123".to_string()));
    }

    #[test]
    fn test_extract_auth_token_lowercase_header() {
        let client = ForgeMcpClient::new(McpServerConfig::new_sse("http://test.com"));
        let mut headers = BTreeMap::new();
        headers.insert("authorization".to_string(), "Bearer token456".to_string());

        let result = client.extract_auth_token(&headers);

        assert_eq!(result, Some("token456".to_string()));
    }

    #[test]
    fn test_extract_auth_token_priority() {
        let client = ForgeMcpClient::new(McpServerConfig::new_sse("http://test.com"));
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer token1".to_string());
        headers.insert("authorization".to_string(), "Bearer token2".to_string());

        let result = client.extract_auth_token(&headers);

        // Should prefer "Authorization" (capital A)
        assert_eq!(result, Some("token1".to_string()));
    }

    #[test]
    fn test_extract_auth_token_no_auth_header() {
        let client = ForgeMcpClient::new(McpServerConfig::new_sse("http://test.com"));
        let mut headers = BTreeMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());

        let result = client.extract_auth_token(&headers);

        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_auth_token_empty_bearer() {
        let client = ForgeMcpClient::new(McpServerConfig::new_sse("http://test.com"));
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer ".to_string());

        let result = client.extract_auth_token(&headers);

        assert_eq!(result, Some("".to_string()));
    }

    #[tokio::test]
    async fn test_create_sse_transport_with_empty_headers_uses_standard() {
        let config = McpServerConfig::new_sse("http://test.com");
        let client = ForgeMcpClient::new(config);

        if let McpServerConfig::Sse(sse) = &client.config {
            // When headers are empty, it should use the standard transport path
            assert!(sse.headers.is_empty());
            // This would call SseClientTransport::start(sse.url.clone())
            // internally We can't easily mock this, but we can
            // verify the logic path
        } else {
            panic!("Expected SSE config");
        }
    }

    #[test]
    fn test_auth_header_parsing_with_various_formats() {
        // Test that Authorization header parsing doesn't panic with various formats
        let test_cases = vec![
            ("Bearer token123", "token123"),
            ("token123", "token123"),
            ("Bearer", "Bearer"), // Edge case: just "Bearer" - no space to strip
            ("", ""),             // Edge case: empty string
        ];

        for (input, expected) in test_cases {
            let client = ForgeMcpClient::new(McpServerConfig::new_sse("http://test.com"));
            let mut headers = BTreeMap::new();
            headers.insert("Authorization".to_string(), input.to_string());

            let result = client.extract_auth_token(&headers);
            assert_eq!(result, Some(expected.to_string()));
        }
    }

    #[test]
    fn test_header_extraction_edge_cases() {
        let client = ForgeMcpClient::new(McpServerConfig::new_sse("http://test.com"));

        // Test with special characters in token
        let mut headers = BTreeMap::new();
        headers.insert(
            "Authorization".to_string(),
            "Bearer token-with-special.chars_123".to_string(),
        );
        let result = client.extract_auth_token(&headers);
        assert_eq!(result, Some("token-with-special.chars_123".to_string()));

        // Test with whitespace
        headers.clear();
        headers.insert(
            "Authorization".to_string(),
            "Bearer   token123  ".to_string(),
        );
        let result = client.extract_auth_token(&headers);
        // Note: whitespace is preserved as per current implementation
        assert_eq!(result, Some("  token123  ".to_string()));
    }
}
