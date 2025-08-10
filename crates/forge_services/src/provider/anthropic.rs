use std::sync::Arc;

use anyhow::Context as _;
use forge_app::HttpClientService;
use forge_app::domain::{
    ChatCompletionMessage, Context, Model, ModelId, ResultStream, Transformer,
};
use forge_app::dto::anthropic::{EventData, ListModelResponse, ReasoningTransform, Request};
use reqwest::Url;
use tracing::debug;

use crate::provider::client::{create_headers, join_url};
use crate::provider::event::into_chat_completion_message;
use crate::provider::utils::format_http_context;

#[derive(Clone)]
pub struct Anthropic<T> {
    http: Arc<T>,
    api_key: String,
    base_url: String,
    anthropic_version: String,
}

impl<H: HttpClientService> Anthropic<H> {
    pub fn new(http: Arc<H>, api_key: String, base_url: String, version: String) -> Self {
        Self { http, api_key, base_url, anthropic_version: version }
    }

    fn get_headers(&self) -> Vec<(String, String)> {
        vec![
            ("x-api-key".to_string(), self.api_key.clone()),
            (
                "anthropic-version".to_string(),
                self.anthropic_version.clone(),
            ),
        ]
    }

    fn url(&self, path: &str) -> anyhow::Result<Url> {
        join_url(&self.base_url, path)
    }
}

impl<T: HttpClientService> Anthropic<T> {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let max_tokens = context.max_tokens.unwrap_or(4000);
        // transform the context to match the request format
        let context = ReasoningTransform.transform(context);

        let request = Request::try_from(context)?
            .model(model.as_str().to_string())
            .stream(true)
            .max_tokens(max_tokens as u64);

        let path = "/messages";
        let url = self.url(path)?;
        debug!(url = %url, model = %model, "Connecting Upstream");

        let json_bytes =
            serde_json::to_vec(&request).with_context(|| "Failed to serialize request")?;

        let source = self
            .http
            .eventsource(
                &url,
                Some(create_headers(self.get_headers())),
                json_bytes.into(),
            )
            .await
            .with_context(|| format_http_context(None, "POST", &url))?;

        let stream = into_chat_completion_message::<EventData>(url, source);

        Ok(Box::pin(stream))
    }

    pub async fn models(&self) -> anyhow::Result<Vec<Model>> {
        let url = join_url(&self.base_url, "models")?;
        debug!(url = %url, "Fetching models");

        let response = self
            .http
            .get(&url, Some(create_headers(self.get_headers())))
            .await
            .with_context(|| format_http_context(None, "GET", &url))
            .with_context(|| "Failed to fetch models")?;

        let status = response.status();
        let ctx_msg = format_http_context(Some(status), "GET", &url);
        let text = response
            .text()
            .await
            .with_context(|| ctx_msg.clone())
            .with_context(|| "Failed to decode response into text")?;

        if status.is_success() {
            let response: ListModelResponse = serde_json::from_str(&text)
                .with_context(|| ctx_msg)
                .with_context(|| "Failed to deserialize models response")?;
            Ok(response.data.into_iter().map(Into::into).collect())
        } else {
            // treat non 200 response as error.
            Err(anyhow::anyhow!(text))
                .with_context(|| ctx_msg)
                .with_context(|| "Failed to fetch the models")
        }
    }
}

#[cfg(test)]
mod tests {

    use bytes::Bytes;
    use forge_app::HttpClientService;
    use forge_app::domain::{
        Context, ContextMessage, ToolCallFull, ToolCallId, ToolChoice, ToolName, ToolOutput,
        ToolResult,
    };
    use reqwest::header::HeaderMap;
    use reqwest_eventsource::EventSource;

    use super::*;
    use crate::provider::mock_server::{MockServer, normalize_ports};

    // Mock implementation of HttpClientService for testing
    #[derive(Clone)]
    struct MockHttpClient {
        client: reqwest::Client,
    }

    impl MockHttpClient {
        fn new() -> Self {
            Self { client: reqwest::Client::new() }
        }
    }

    #[async_trait::async_trait]
    impl HttpClientService for MockHttpClient {
        async fn get(
            &self,
            url: &reqwest::Url,
            headers: Option<HeaderMap>,
        ) -> anyhow::Result<reqwest::Response> {
            let mut request = self.client.get(url.clone());
            if let Some(headers) = headers {
                request = request.headers(headers);
            }
            Ok(request.send().await?)
        }

        async fn post(&self, _url: &Url, _body: Bytes) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn delete(&self, _url: &Url) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn eventsource(
            &self,
            _url: &Url,
            _headers: Option<HeaderMap>,
            _body: Bytes,
        ) -> anyhow::Result<EventSource> {
            // For now, return an error since eventsource is not used in the failing tests
            Err(anyhow::anyhow!("EventSource not implemented in mock"))
        }
    }

    fn create_anthropic(base_url: &str) -> anyhow::Result<Anthropic<MockHttpClient>> {
        Ok(Anthropic::new(
            Arc::new(MockHttpClient::new()),
            "sk-test-key".to_string(),
            base_url.to_string(),
            "2023-06-01".to_string(),
        ))
    }

    fn create_mock_models_response() -> serde_json::Value {
        serde_json::json!({
            "data": [
                {
                    "type": "model",
                    "id": "claude-3-5-sonnet-20241022",
                    "display_name": "Claude 3.5 Sonnet (New)",
                    "created_at": "2024-10-22T00:00:00Z"
                },
                {
                    "type": "model",
                    "id": "claude-3-5-haiku-20241022",
                    "display_name": "Claude 3.5 Haiku",
                    "created_at": "2024-10-22T00:00:00Z"
                }
            ],
            "has_more": false,
            "first_id": "claude-3-5-sonnet-20241022",
            "last_id": "claude-3-opus-20240229"
        })
    }

    fn create_error_response(message: &str, code: u16) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "code": code,
                "message": message
            }
        })
    }

    fn create_empty_response() -> serde_json::Value {
        serde_json::json!({
            "data": [],
        })
    }

    #[tokio::test]
    async fn test_url_for_models() {
        let anthropic = Anthropic::new(
            Arc::new(MockHttpClient::new()),
            "sk-some-key".to_string(),
            "https://api.anthropic.com/v1/".to_string(),
            "v1".to_string(),
        );
        assert_eq!(
            anthropic.url("/models").unwrap().as_str(),
            "https://api.anthropic.com/v1/models"
        );
    }

    #[tokio::test]
    async fn test_request_conversion() {
        let model_id = ModelId::new("gpt-4");
        let context = Context::default()
            .add_message(ContextMessage::system(
                "You're expert at math, so you should resolve all user queries.",
            ))
            .add_message(ContextMessage::user(
                "what's 2 + 2 ?",
                model_id.clone().into(),
            ))
            .add_message(ContextMessage::assistant(
                "here is the system call.",
                None,
                Some(vec![ToolCallFull {
                    name: ToolName::new("math"),
                    call_id: Some(ToolCallId::new("math-1")),
                    arguments: serde_json::json!({"expression": "2 + 2"}),
                }]),
            ))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("math"),
                call_id: Some(ToolCallId::new("math-1")),
                output: ToolOutput::text(serde_json::json!({"result": 4}).to_string()),
            }])
            .tool_choice(ToolChoice::Call(ToolName::new("math")));
        let request = Request::try_from(context)
            .unwrap()
            .model("sonnet-3.5".to_string())
            .stream(true)
            .max_tokens(4000u64);
        insta::assert_snapshot!(serde_json::to_string_pretty(&request).unwrap());
    }

    #[tokio::test]
    async fn test_fetch_models_success() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_mock_models_response(), 200)
            .await;
        let anthropic = create_anthropic(&fixture.url())?;
        let actual = anthropic.models().await?;

        mock.assert_async().await;

        // Verify we got the expected models
        assert_eq!(actual.len(), 2);
        insta::assert_json_snapshot!(actual);
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_http_error_status() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_error_response("Invalid API key", 401), 401)
            .await;

        let anthropic = create_anthropic(&fixture.url())?;
        let actual = anthropic.models().await;

        mock.assert_async().await;

        // Verify that we got an error
        assert!(actual.is_err());
        insta::assert_snapshot!(normalize_ports(format!("{:#?}", actual.unwrap_err())));
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_server_error() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_error_response("Internal Server Error", 500), 500)
            .await;

        let anthropic = create_anthropic(&fixture.url())?;
        let actual = anthropic.models().await;

        mock.assert_async().await;

        // Verify that we got an error
        assert!(actual.is_err());
        insta::assert_snapshot!(normalize_ports(format!("{:#?}", actual.unwrap_err())));

        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_empty_response() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture.mock_models(create_empty_response(), 200).await;

        let anthropic = create_anthropic(&fixture.url())?;
        let actual = anthropic.models().await?;

        mock.assert_async().await;
        assert!(actual.is_empty());
        Ok(())
    }
}
