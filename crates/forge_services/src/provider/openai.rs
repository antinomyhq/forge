use std::sync::Arc;

use anyhow::{Context as _, Result};
use forge_app::HttpClientService;
use forge_app::domain::{
    ChatCompletionMessage, Context as ChatContext, ModelId, ProviderId, ResultStream, Transformer,
};
use forge_app::dto::openai::{ListModelResponse, ProviderPipeline, Request, Response};
use forge_domain::Provider;
use lazy_static::lazy_static;
use reqwest::header::AUTHORIZATION;
use tracing::{debug, info};
use url::Url;

use crate::provider::client::{create_headers, join_url};
use crate::provider::event::into_chat_completion_message;
use crate::provider::utils::{format_http_context, sanitize_headers};

#[derive(Clone)]
pub struct OpenAIProvider<H> {
    provider: Provider<Url>,
    http: Arc<H>,
}

impl<H: HttpClientService> OpenAIProvider<H> {
    pub fn new(provider: Provider<Url>, http: Arc<H>) -> Self {
        Self { provider, http }
    }

    // OpenRouter optional headers ref: https://openrouter.ai/docs/api-reference/overview#headers
    // - `HTTP-Referer`: Identifies your app on openrouter.ai
    // - `X-Title`: Sets/modifies your app's title
    fn get_headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if let Some(api_key) = self
            .provider
            .credential
            .as_ref()
            .map(|c| match &c.auth_details {
                forge_domain::AuthDetails::ApiKey(key) => key.as_str(),
                forge_domain::AuthDetails::OAuthWithApiKey { api_key, .. } => api_key.as_str(),
                forge_domain::AuthDetails::OAuth { tokens, .. } => tokens.access_token.as_str(),
            })
        {
            headers.push((AUTHORIZATION.to_string(), format!("Bearer {api_key}")));
        }
        self.provider
            .auth_methods
            .iter()
            .for_each(|method| match method {
                forge_domain::AuthMethod::ApiKey => {}
                forge_domain::AuthMethod::OAuthDevice(oauth_config) => {
                    if let Some(custom_headers) = &oauth_config.custom_headers {
                        custom_headers.iter().for_each(|(k, v)| {
                            headers.push((k.clone(), v.clone()));
                        });
                    }
                }
                forge_domain::AuthMethod::OAuthCode(oauth_config) => {
                    if let Some(custom_headers) = &oauth_config.custom_headers {
                        custom_headers.iter().for_each(|(k, v)| {
                            headers.push((k.clone(), v.clone()));
                        });
                    }
                }
            });
        headers
    }

    /// Creates headers including Session-Id for zai and zai_coding providers
    fn get_headers_with_request(&self, request: &Request) -> Vec<(String, String)> {
        let mut headers = self.get_headers();
        // Add Session-Id header for zai and zai_coding providers
        if let Some(session_id) = &request.session_id
            && (self.provider.id == ProviderId::ZAI || self.provider.id == ProviderId::ZAI_CODING)
        {
            headers.push(("Session-Id".to_string(), session_id.clone()));
            debug!(
                provider = %self.provider.url,
                session_id = %session_id,
                "Added Session-Id header for zai provider"
            );
        }

        headers
    }

    async fn inner_chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let mut request = Request::from(context).model(model.clone());
        let mut pipeline = ProviderPipeline::new(&self.provider);
        request = pipeline.transform(request);

        let url = self.provider.url.clone();
        let headers = create_headers(self.get_headers_with_request(&request));

        info!(
            url = %url,
            model = %model,
            headers = ?sanitize_headers(&headers),
            message_count = %request.message_count(),
            message_cache_count = %request.message_cache_count(),
            "Connecting Upstream"
        );

        let json_bytes =
            serde_json::to_vec(&request).with_context(|| "Failed to serialize request")?;

        let es = self
            .http
            .eventsource(&url, Some(headers), json_bytes.into())
            .await
            .with_context(|| format_http_context(None, "POST", &url))?;

        let stream = into_chat_completion_message::<Response>(url, es);

        Ok(Box::pin(stream))
    }

    async fn inner_models(&self) -> Result<Vec<forge_app::domain::Model>> {
        // For Vertex AI, load models from static JSON file using VertexProvider logic
        if self.provider.id == ProviderId::VERTEX_AI {
            debug!("Loading Vertex AI models from static JSON file");
            Ok(self.inner_vertex_models())
        } else {
            let models = self
                .provider
                .models()
                .ok_or_else(|| anyhow::anyhow!("Provider models configuration is required"))?;

            match models {
                forge_domain::ModelSource::Url(url) => {
                    debug!(url = %url, "Fetching models");
                    match self.fetch_models(url.as_str()).await {
                        Err(error) => {
                            tracing::error!(error = ?error, "Failed to fetch models");
                            anyhow::bail!(error)
                        }
                        Ok(response) => {
                            let data: ListModelResponse = serde_json::from_str(&response)
                                .with_context(|| format_http_context(None, "GET", url))
                                .with_context(|| "Failed to deserialize models response")?;
                            Ok(data.data.into_iter().map(Into::into).collect())
                        }
                    }
                }
                forge_domain::ModelSource::Hardcoded(models) => {
                    debug!("Using hardcoded models");
                    Ok(models.clone())
                }
            }
        }
    }

    async fn fetch_models(&self, url: &str) -> Result<String, anyhow::Error> {
        let headers = create_headers(self.get_headers());
        let url = join_url(url, "")?;
        info!(method = "GET", url = %url, headers = ?sanitize_headers(&headers), "Fetching Models");

        let response = self
            .http
            .get(&url, Some(headers))
            .await
            .with_context(|| format_http_context(None, "GET", &url))
            .with_context(|| "Failed to fetch the models")?;

        let status = response.status();
        let ctx_message = format_http_context(Some(status), "GET", &url);

        let response_text = response
            .text()
            .await
            .with_context(|| ctx_message.clone())
            .with_context(|| "Failed to decode response into text")?;

        if status.is_success() {
            Ok(response_text)
        } else {
            Err(anyhow::anyhow!(response_text))
                .with_context(|| ctx_message)
                .with_context(|| "Failed to fetch the models")
        }
    }

    /// Load Vertex AI models from static JSON file
    fn inner_vertex_models(&self) -> Vec<forge_app::domain::Model> {
        lazy_static! {
            static ref VERTEX_MODELS: Vec<forge_app::domain::Model> = {
                let models =
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../vertex.json"));
                serde_json::from_str(models).unwrap()
            };
        }
        VERTEX_MODELS.clone()
    }
}

impl<T: HttpClientService> OpenAIProvider<T> {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.inner_chat(model, context).await
    }

    pub async fn models(&self) -> Result<Vec<forge_app::domain::Model>> {
        self.inner_models().await
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use anyhow::Context;
    use bytes::Bytes;
    use forge_app::HttpClientService;
    use forge_app::domain::{Provider, ProviderId, ProviderResponse};
    use reqwest::header::HeaderMap;
    use reqwest_eventsource::EventSource;
    use url::Url;

    use super::*;
    use crate::provider::mock_server::{MockServer, normalize_ports};

    // Test helper functions
    fn make_credential(provider_id: ProviderId, key: &str) -> Option<forge_domain::AuthCredential> {
        Some(forge_domain::AuthCredential {
            id: provider_id,
            auth_details: forge_domain::AuthDetails::ApiKey(forge_domain::ApiKey::from(
                key.to_string(),
            )),
            url_params: HashMap::new(),
        })
    }

    fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            credential: make_credential(ProviderId::OPENAI, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.openai.com/v1/models").unwrap(),
            )),
        }
    }

    fn zai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ZAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            credential: make_credential(ProviderId::ZAI, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.z.ai/api/paas/v4/models").unwrap(),
            )),
        }
    }

    fn zai_coding(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ZAI_CODING,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            credential: make_credential(ProviderId::ZAI_CODING, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.z.ai/api/paas/v4/models").unwrap(),
            )),
        }
    }

    fn anthropic(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ANTHROPIC,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::Anthropic),
            url: Url::parse("https://api.anthropic.com/v1/messages").unwrap(),
            credential: make_credential(ProviderId::ANTHROPIC, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.anthropic.com/v1/models").unwrap(),
            )),
        }
    }

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

        async fn post(
            &self,
            _url: &reqwest::Url,
            _body: Bytes,
        ) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn delete(&self, _url: &reqwest::Url) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn eventsource(
            &self,
            _url: &reqwest::Url,
            _headers: Option<HeaderMap>,
            _body: Bytes,
        ) -> anyhow::Result<EventSource> {
            unimplemented!()
        }
    }

    fn create_provider(base_url: &str) -> anyhow::Result<OpenAIProvider<MockHttpClient>> {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: reqwest::Url::parse(base_url)?,
            credential: make_credential(ProviderId::OPENAI, "test-api-key"),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                reqwest::Url::parse(base_url)?.join("models")?,
            )),
        };

        Ok(OpenAIProvider::new(
            provider,
            Arc::new(MockHttpClient::new()),
        ))
    }

    fn create_mock_models_response() -> serde_json::Value {
        serde_json::json!({
            "data": [
                {
                    "id": "model-1",
                    "name": "Test Model 1",
                    "description": "A test model",
                    "context_length": 4096,
                    "supported_parameters": ["tools", "supports_parallel_tool_calls"]
                },
                {
                    "id": "model-2",
                    "name": "Test Model 2",
                    "description": "Another test model",
                    "context_length": 8192,
                    "supported_parameters": ["tools"]
                }
            ]
        })
    }

    fn create_error_response(message: &str, code: u16) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "message": message,
                "code": code
            }
        })
    }

    fn create_empty_response() -> serde_json::Value {
        serde_json::json!({ "data": [] })
    }

    #[tokio::test]
    async fn test_fetch_models_success() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_mock_models_response(), 200)
            .await;
        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await?;

        mock.assert_async().await;
        insta::assert_json_snapshot!(actual);
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_http_error_status() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_error_response("Invalid API key", 401), 401)
            .await;

        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await;

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

        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await;

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

        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await?;

        mock.assert_async().await;
        assert!(actual.is_empty());
        Ok(())
    }

    #[test]
    fn test_error_deserialization() -> Result<()> {
        let content = serde_json::to_string(&serde_json::json!({
          "error": {
            "message": "This endpoint's maximum context length is 16384 tokens",
            "code": 400
          }
        }))
        .unwrap();
        let message = serde_json::from_str::<Response>(&content)
            .with_context(|| "Failed to parse response")?;
        let message = ChatCompletionMessage::try_from(message.clone());

        assert!(message.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_detailed_error_message_included() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let detailed_error = create_error_response(
            "Authentication failed: API key is invalid or expired. Please check your API key.",
            401,
        );
        let mock = fixture.mock_models(detailed_error, 401).await;

        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await;

        mock.assert_async().await;
        assert!(actual.is_err());
        insta::assert_snapshot!(normalize_ports(format!("{:#?}", actual.unwrap_err())));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_zai_provider() -> anyhow::Result<()> {
        let provider = zai("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request with session_id
        let request = Request {
            session_id: Some("test-conversation-id".to_string()),
            ..Default::default()
        };

        let headers = openai_provider.get_headers_with_request(&request);

        // Should have Authorization and Session-Id headers
        assert_eq!(headers.len(), 2);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "Session-Id" && v == "test-conversation-id")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_zai_coding_provider() -> anyhow::Result<()> {
        let provider = zai_coding("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request with session_id
        let request = Request {
            session_id: Some("test-conversation-id".to_string()),
            ..Default::default()
        };

        let headers = openai_provider.get_headers_with_request(&request);

        // Should have Authorization and Session-Id headers
        assert_eq!(headers.len(), 2);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "Session-Id" && v == "test-conversation-id")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_openai_provider() -> anyhow::Result<()> {
        let provider = openai("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request with session_id
        let request = Request {
            session_id: Some("test-conversation-id".to_string()),
            ..Default::default()
        };

        let headers = openai_provider.get_headers_with_request(&request);

        // Should only have Authorization header (no Session-Id for non-zai providers)
        assert_eq!(headers.len(), 1);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(!headers.iter().any(|(k, _)| k == "Session-Id"));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_zai_provider_no_session_id() -> anyhow::Result<()> {
        let provider = zai("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request without session_id
        let request = Request::default();

        let headers = openai_provider.get_headers_with_request(&request);

        // Should only have Authorization header (no Session-Id when session_id is None)
        assert_eq!(headers.len(), 1);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(!headers.iter().any(|(k, _)| k == "Session-Id"));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_anthropic_provider() -> anyhow::Result<()> {
        let provider = anthropic("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request with session_id
        let request = Request {
            session_id: Some("test-conversation-id".to_string()),
            ..Default::default()
        };

        let headers = openai_provider.get_headers_with_request(&request);

        // Should only have Authorization header (no Session-Id for Anthropic providers)
        assert_eq!(headers.len(), 1);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(!headers.iter().any(|(k, _)| k == "Session-Id"));
        Ok(())
    }

    #[test]
    fn test_get_headers_fallback() -> anyhow::Result<()> {
        let provider = zai("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        let headers = openai_provider.get_headers();

        // Should only have Authorization header (fallback method doesn't add
        // Session-Id)
        assert_eq!(headers.len(), 1);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(!headers.iter().any(|(k, _)| k == "Session-Id"));
        Ok(())
    }
}
