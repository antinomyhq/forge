use std::sync::Arc;

use anyhow::Context as _;
use derive_setters::Setters;
use forge_app::HttpInfra;
use forge_app::domain::{
    ChatCompletionMessage, Context, Model, ModelId, ResultStream, RetryConfig, Transformer,
};
use forge_app::dto::anthropic::{
    AuthSystemMessage, CapitalizeToolNames, DropInvalidToolUse, EnforceStrictObjectSchema,
    EventData, ListModelResponse, ReasoningTransform, Request, SetCache,
};
use forge_domain::{ChatRepository, Provider};
use reqwest::Url;
use tokio_stream::StreamExt;
use tracing::debug;

use crate::provider::event::into_chat_completion_message;
use crate::provider::retry::into_retry;
use crate::provider::utils::{create_headers, format_http_context};

#[derive(Clone)]
struct Anthropic<T> {
    http: Arc<T>,
    api_key: String,
    chat_url: Url,
    models: forge_domain::ModelSource<Url>,
    anthropic_version: String,
    use_oauth: bool,
}

impl<H: HttpInfra> Anthropic<H> {
    pub fn new(
        http: Arc<H>,
        api_key: String,
        chat_url: Url,
        models: forge_domain::ModelSource<Url>,
        version: String,
        use_oauth: bool,
    ) -> Self {
        Self {
            http,
            api_key,
            chat_url,
            models,
            anthropic_version: version,
            use_oauth,
        }
    }

    fn get_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![(
            "anthropic-version".to_string(),
            self.anthropic_version.clone(),
        )];

        // Use Authorization: Bearer for OAuth, x-api-key for API key auth
        if self.use_oauth {
            headers.push((
                "authorization".to_string(),
                format!("Bearer {}", self.api_key),
            ));
            // OAuth requires multiple beta flags including structured outputs
            headers.push((
                "anthropic-beta".to_string(),
                "claude-code-20250219,oauth-2025-04-20,interleaved-thinking-2025-05-14,structured-outputs-2025-11-13".to_string(),
            ));
        } else {
            headers.push(("x-api-key".to_string(), self.api_key.clone()));
            // API key auth also needs beta flags for structured outputs and thinking
            headers.push((
                "anthropic-beta".to_string(),
                "interleaved-thinking-2025-05-14,structured-outputs-2025-11-13".to_string(),
            ));
        }

        headers
    }
}

impl<T: HttpInfra> Anthropic<T> {
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
            .max_tokens(max_tokens as u64);

        let request = AuthSystemMessage::default()
            .when(|_| self.use_oauth)
            .pipe(CapitalizeToolNames)
            .pipe(DropInvalidToolUse)
            .pipe(EnforceStrictObjectSchema)
            .pipe(SetCache)
            .transform(request);
        let url = &self.chat_url;
        debug!(url = %url, model = %model, "Connecting Upstream");

        let json_bytes =
            serde_json::to_vec(&request).with_context(|| "Failed to serialize request")?;

        let source = self
            .http
            .http_eventsource(
                url,
                Some(create_headers(self.get_headers())),
                json_bytes.into(),
            )
            .await
            .with_context(|| format_http_context(None, "POST", url))?;

        let stream = into_chat_completion_message::<EventData>(url.clone(), source);

        Ok(Box::pin(stream))
    }

    pub async fn models(&self) -> anyhow::Result<Vec<Model>> {
        match &self.models {
            forge_domain::ModelSource::Url(url) => {
                debug!(url = %url, "Fetching models");

                let response = self
                    .http
                    .http_get(url, Some(create_headers(self.get_headers())))
                    .await
                    .with_context(|| format_http_context(None, "GET", url))
                    .with_context(|| "Failed to fetch models")?;

                let status = response.status();
                let ctx_msg = format_http_context(Some(status), "GET", url);
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
            forge_domain::ModelSource::Hardcoded(models) => {
                debug!("Using hardcoded models");
                Ok(models.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use bytes::Bytes;
    use forge_app::HttpInfra;
    use forge_app::domain::{
        Context, ContextMessage, ToolCallFull, ToolCallId, ToolChoice, ToolName, ToolOutput,
        ToolResult,
    };
    use reqwest::header::HeaderMap;
    use reqwest_eventsource::EventSource;

    use super::*;
    use crate::provider::mock_server::{MockServer, normalize_ports};

    // Mock implementation of HttpInfra for testing
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
    impl HttpInfra for MockHttpClient {
        async fn http_get(
            &self,
            url: &Url,
            headers: Option<HeaderMap>,
        ) -> anyhow::Result<reqwest::Response> {
            let mut request = self.client.get(url.clone());
            if let Some(headers) = headers {
                request = request.headers(headers);
            }
            Ok(request.send().await?)
        }

        async fn http_post(&self, _url: &Url, _body: Bytes) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn http_delete(&self, _url: &Url) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn http_eventsource(
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
        let chat_url = Url::parse(base_url)?.join("messages")?;
        let model_url = Url::parse(base_url)?.join("models")?;
        Ok(Anthropic::new(
            Arc::new(MockHttpClient::new()),
            "sk-test-key".to_string(),
            chat_url,
            forge_domain::ModelSource::Url(model_url),
            "2023-06-01".to_string(),
            false,
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
        let chat_url = Url::parse("https://api.anthropic.com/v1/messages").unwrap();
        let model_url = Url::parse("https://api.anthropic.com/v1/models").unwrap();
        let anthropic = Anthropic::new(
            Arc::new(MockHttpClient::new()),
            "sk-some-key".to_string(),
            chat_url,
            forge_domain::ModelSource::Url(model_url.clone()),
            "v1".to_string(),
            false,
        );
        match &anthropic.models {
            forge_domain::ModelSource::Url(url) => {
                assert_eq!(url.as_str(), "https://api.anthropic.com/v1/models");
            }
            _ => panic!("Expected Models::Url variant"),
        }
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
                    arguments: serde_json::json!({"expression": "2 + 2"}).into(),
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

    #[test]
    fn test_get_headers_with_api_key_includes_beta_flags() {
        let chat_url = Url::parse("https://api.anthropic.com/v1/messages").unwrap();
        let model_url = Url::parse("https://api.anthropic.com/v1/models").unwrap();
        let fixture = Anthropic::new(
            Arc::new(MockHttpClient::new()),
            "sk-test-key".to_string(),
            chat_url,
            forge_domain::ModelSource::Url(model_url),
            "2023-06-01".to_string(),
            false, // API key auth (not OAuth)
        );

        let actual = fixture.get_headers();

        // Should contain anthropic-version header
        assert!(
            actual
                .iter()
                .any(|(k, v)| k == "anthropic-version" && v == "2023-06-01")
        );

        // Should contain x-api-key header (not authorization)
        assert!(
            actual
                .iter()
                .any(|(k, v)| k == "x-api-key" && v == "sk-test-key")
        );

        // Should contain anthropic-beta header with structured outputs support
        let beta_header = actual.iter().find(|(k, _)| k == "anthropic-beta");
        assert!(
            beta_header.is_some(),
            "anthropic-beta header should be present for API key auth"
        );

        let (_, beta_value) = beta_header.unwrap();
        assert!(
            beta_value.contains("structured-outputs-2025-11-13"),
            "Beta header should include structured-outputs flag"
        );
        assert!(
            beta_value.contains("interleaved-thinking-2025-05-14"),
            "Beta header should include interleaved-thinking flag"
        );
    }

    #[test]
    fn test_get_headers_with_oauth_includes_beta_flags() {
        let chat_url = Url::parse("https://api.anthropic.com/v1/messages").unwrap();
        let model_url = Url::parse("https://api.anthropic.com/v1/models").unwrap();
        let fixture = Anthropic::new(
            Arc::new(MockHttpClient::new()),
            "oauth-token".to_string(),
            chat_url,
            forge_domain::ModelSource::Url(model_url),
            "2023-06-01".to_string(),
            true, // OAuth auth
        );

        let actual = fixture.get_headers();

        // Should contain anthropic-version header
        assert!(
            actual
                .iter()
                .any(|(k, v)| k == "anthropic-version" && v == "2023-06-01")
        );

        // Should contain authorization header (not x-api-key)
        assert!(
            actual
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer oauth-token")
        );

        // Should contain anthropic-beta header with structured outputs support
        let beta_header = actual.iter().find(|(k, _)| k == "anthropic-beta");
        assert!(
            beta_header.is_some(),
            "anthropic-beta header should be present for OAuth"
        );

        let (_, beta_value) = beta_header.unwrap();
        assert!(
            beta_value.contains("structured-outputs-2025-11-13"),
            "Beta header should include structured-outputs flag"
        );
        assert!(
            beta_value.contains("oauth-2025-04-20"),
            "Beta header should include oauth flag for OAuth auth"
        );
    }
}

/// Repository for Anthropic provider responses
#[derive(Setters)]
#[setters(strip_option, into)]
pub struct AnthropicResponseRepository<F> {
    infra: Arc<F>,
    retry_config: Arc<RetryConfig>,
}

impl<F> AnthropicResponseRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, retry_config: Arc::new(RetryConfig::default()) }
    }
}

impl<F: HttpInfra> AnthropicResponseRepository<F> {
    /// Creates an Anthropic client from a provider configuration
    fn create_client(&self, provider: &Provider<Url>) -> anyhow::Result<Anthropic<F>> {
        let chat_url = provider.url.clone();
        let models = provider
            .models
            .clone()
            .context("Anthropic requires models configuration")?;
        let creds = provider
            .credential
            .as_ref()
            .context("Anthropic provider requires credentials")?
            .auth_details
            .clone();

        let (key, is_oauth) = match creds {
            forge_domain::AuthDetails::ApiKey(api_key) => (api_key.as_str().to_string(), false),
            forge_domain::AuthDetails::OAuth { tokens, .. } => {
                (tokens.access_token.as_str().to_string(), true)
            }
            _ => anyhow::bail!("Unsupported authentication method for Anthropic provider"),
        };

        Ok(Anthropic::new(
            self.infra.clone(),
            key,
            chat_url,
            models,
            "2023-06-01".to_string(),
            is_oauth,
        ))
    }
}

#[async_trait::async_trait]
impl<F: HttpInfra + 'static> ChatRepository for AnthropicResponseRepository<F> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let retry_config = self.retry_config.clone();
        let provider_client = self.create_client(&provider)?;

        let stream = provider_client
            .chat(model_id, context)
            .await
            .map_err(|e| into_retry(e, &retry_config))?;

        Ok(Box::pin(stream.map(move |item| {
            item.map_err(|e| into_retry(e, &retry_config))
        })))
    }

    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        let retry_config = self.retry_config.clone();
        let provider_client = self.create_client(&provider)?;

        provider_client
            .models()
            .await
            .map_err(|e| into_retry(e, &retry_config))
            .context("Failed to fetch models from Anthropic provider")
    }
}
