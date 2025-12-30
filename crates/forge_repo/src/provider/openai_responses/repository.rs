use std::sync::Arc;

use anyhow::Context as _;
use async_openai::types::responses as oai;
use derive_setters::Setters;
use forge_app::HttpInfra;
use forge_app::domain::{
    ChatCompletionMessage, Context as ChatContext, Model, ModelId, ResultStream, RetryConfig,
};
use forge_domain::{ChatRepository, Provider};
use futures::StreamExt;
use reqwest::header::AUTHORIZATION;
use tracing::info;
use url::Url;

use crate::provider::FromDomain;
use crate::provider::retry::into_retry;
use crate::provider::utils::{create_headers, format_http_context, sanitize_headers};

#[derive(Clone)]
pub(super) struct OpenAIResponsesProvider<H> {
    provider: Provider<Url>,
    http: Arc<H>,
    api_base: Url,
    responses_url: Url,
}

impl<H: HttpInfra> OpenAIResponsesProvider<H> {
    /// Creates a new OpenAI Responses provider
    ///
    /// # Panics
    ///
    /// Panics if the provider URL cannot be converted to an API base URL
    pub fn new(provider: Provider<Url>, http: Arc<H>) -> Self {
        let api_base = api_base_from_endpoint_url(&provider.url)
            .expect("Failed to derive API base URL from provider endpoint");
        let responses_url = responses_endpoint_from_api_base(&api_base);

        Self { provider, http, api_base, responses_url }
    }

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
}

impl<T: HttpInfra> OpenAIResponsesProvider<T> {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let headers = create_headers(self.get_headers());
        let mut request = oai::CreateResponse::from_domain(context)?;
        request.model = Some(model.as_str().to_string());

        info!(
            url = %self.responses_url,
            base_url = %self.api_base,
            model = %model,
            headers = ?sanitize_headers(&headers),
            message_count = %request_message_count(&request),
            "Connecting Upstream (Codex via Responses API)"
        );

        let json_bytes = serde_json::to_vec(&request)
            .with_context(|| "Failed to serialize OpenAI Responses request")?;

        let source = self
            .http
            .http_eventsource(&self.responses_url, Some(headers), json_bytes.into())
            .await
            .with_context(|| format_http_context(None, "POST", &self.responses_url))?;

        // Parse SSE stream into domain messages and convert to domain type
        use reqwest_eventsource::Event;
        let event_stream = source
            .take_while(|message| {
                let should_continue =
                    !matches!(message, Err(reqwest_eventsource::Error::StreamEnded));
                async move { should_continue }
            })
            .filter_map(|event_result| async move {
                match event_result {
                    Ok(Event::Open) => None,
                    Ok(Event::Message(msg)) if ["[DONE]", ""].contains(&msg.data.as_str()) => None,
                    Ok(Event::Message(msg)) => {
                        // Parse the SSE event data as ResponseStreamEvent
                        let result = serde_json::from_str::<oai::ResponseStreamEvent>(&msg.data)
                            .with_context(|| format!("Failed to parse SSE event: {}", msg.data));
                        Some(result)
                    }
                    Err(reqwest_eventsource::Error::StreamEnded) => None,
                    Err(e) => Some(Err(anyhow::Error::from(e))),
                }
            });

        // Convert to domain messages using the existing conversion logic
        use crate::provider::IntoDomain;
        let stream: std::pin::Pin<
            Box<dyn futures::Stream<Item = anyhow::Result<oai::ResponseStreamEvent>> + Send>,
        > = Box::pin(event_stream);
        let stream = stream.into_domain().unwrap();

        Ok(stream)
    }
}

/// Derives an API base URL suitable for OpenAI Responses API from a configured
/// endpoint URL.
///
/// For Codex/Responses usage we only need the host and the `/v1` prefix.
/// Any path on the incoming endpoint is ignored in favor of `/v1`.
fn api_base_from_endpoint_url(endpoint: &Url) -> anyhow::Result<Url> {
    let mut base = endpoint.clone();
    base.set_path("/v1");
    base.set_query(None);
    base.set_fragment(None);
    Ok(base)
}

fn responses_endpoint_from_api_base(api_base: &Url) -> Url {
    let mut url = api_base.clone();

    let mut path = api_base.path().trim_end_matches('/').to_string();
    path.push_str("/responses");

    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);

    url
}

fn request_message_count(request: &oai::CreateResponse) -> usize {
    match &request.input {
        oai::InputParam::Text(_) => 1,
        oai::InputParam::Items(items) => items.len(),
    }
}

/// Repository for OpenAI Codex models using the Responses API
///
/// Handles OpenAI's Codex models (e.g., gpt-5.1-codex, codex-mini-latest)
/// which use the Responses API instead of the standard Chat Completions API.
#[derive(Setters)]
#[setters(strip_option, into)]
pub struct OpenAIResponsesResponseRepository<F> {
    infra: Arc<F>,
    retry_config: Arc<RetryConfig>,
}

impl<F> OpenAIResponsesResponseRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, retry_config: Arc::new(RetryConfig::default()) }
    }
}

#[async_trait::async_trait]
impl<F: HttpInfra + 'static> ChatRepository for OpenAIResponsesResponseRepository<F> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: ChatContext,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let retry_config = self.retry_config.clone();
        let provider_client: OpenAIResponsesProvider<F> =
            OpenAIResponsesProvider::new(provider, self.infra.clone());
        let stream = provider_client
            .chat(model_id, context)
            .await
            .map_err(|e| into_retry(e, &retry_config))?;

        Ok(Box::pin(stream.map(move |item| {
            item.map_err(|e| into_retry(e, &retry_config))
        })))
    }

    async fn models(&self, _provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        // Codex models don't support model listing via the Responses API
        // Return empty list or hardcoded models
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_app::domain::{
        Content, Context as ChatContext, ContextMessage, FinishReason, ModelId, Provider,
        ProviderId, ProviderResponse,
    };
    use tokio_stream::StreamExt;
    use url::Url;

    use super::*;
    use crate::provider::mock_server::MockServer;

    fn make_credential(provider_id: ProviderId, key: &str) -> Option<forge_domain::AuthCredential> {
        Some(forge_domain::AuthCredential {
            id: provider_id,
            auth_details: forge_domain::AuthDetails::ApiKey(forge_domain::ApiKey::from(
                key.to_string(),
            )),
            url_params: HashMap::new(),
        })
    }

    fn openai_responses(key: &str, url: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse(url).unwrap(),
            credential: make_credential(ProviderId::OPENAI, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: None,
        }
    }

    /// Test fixture for creating a mock HTTP client.
    #[derive(Clone)]
    struct MockHttpClient {
        client: reqwest::Client,
    }

    #[async_trait::async_trait]
    impl HttpInfra for MockHttpClient {
        async fn http_get(
            &self,
            url: &reqwest::Url,
            headers: Option<reqwest::header::HeaderMap>,
        ) -> anyhow::Result<reqwest::Response> {
            let mut request = self.client.get(url.clone());
            if let Some(headers) = headers {
                request = request.headers(headers);
            }
            Ok(request.send().await?)
        }

        async fn http_post(
            &self,
            _url: &reqwest::Url,
            _body: bytes::Bytes,
        ) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn http_delete(&self, _url: &reqwest::Url) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn http_eventsource(
            &self,
            url: &reqwest::Url,
            headers: Option<reqwest::header::HeaderMap>,
            body: bytes::Bytes,
        ) -> anyhow::Result<reqwest_eventsource::EventSource> {
            let mut request = self.client.post(url.clone()).body(body);
            if let Some(headers) = headers {
                request = request.headers(headers);
            }
            Ok(reqwest_eventsource::EventSource::new(request)?)
        }
    }

    /// Test fixture for creating a sample OpenAI Responses API response.
    fn openai_response_fixture() -> serde_json::Value {
        serde_json::json!({
            "created_at": 0,
            "id": "resp_1",
            "model": "codex-mini-latest",
            "object": "response",
            "output": [{
                "type": "message",
                "id": "msg_1",
                "role": "assistant",
                "status": "completed",
                "content": [{
                    "type": "output_text",
                    "text": "hello",
                    "annotations": [],
                    "logprobs": null
                }]
            }],
            "status": "completed",
            "usage": {
                "input_tokens": 1,
                "output_tokens": 1,
                "total_tokens": 2,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens_details": {"reasoning_tokens": 0}
            }
        })
    }

    #[test]
    fn test_api_base_from_endpoint_url_trims_expected_suffixes() -> anyhow::Result<()> {
        let openai_endpoint = Url::parse("https://api.openai.com/v1/chat/completions")?;
        let openai_base = api_base_from_endpoint_url(&openai_endpoint)?;
        assert_eq!(openai_base.as_str(), "https://api.openai.com/v1");

        let copilot_endpoint = Url::parse("https://api.githubcopilot.com/chat/completions")?;
        let copilot_base = api_base_from_endpoint_url(&copilot_endpoint)?;
        assert_eq!(copilot_base.as_str(), "https://api.githubcopilot.com/v1");

        Ok(())
    }

    #[test]
    fn test_api_base_from_endpoint_url_removes_query_and_fragment() -> anyhow::Result<()> {
        let url = Url::parse("https://api.openai.com/v1/path?query=1#fragment")?;
        let base = api_base_from_endpoint_url(&url)?;
        assert_eq!(base.as_str(), "https://api.openai.com/v1");
        assert!(base.query().is_none());
        assert!(base.fragment().is_none());

        Ok(())
    }

    #[test]
    fn test_responses_endpoint_from_api_base() -> anyhow::Result<()> {
        let api_base = Url::parse("https://api.openai.com/v1")?;
        let endpoint = responses_endpoint_from_api_base(&api_base);
        assert_eq!(endpoint.as_str(), "https://api.openai.com/v1/responses");

        let api_base = Url::parse("https://api.githubcopilot.com/v1/")?;
        let endpoint = responses_endpoint_from_api_base(&api_base);
        assert_eq!(
            endpoint.as_str(),
            "https://api.githubcopilot.com/v1/responses"
        );

        Ok(())
    }

    #[test]
    fn test_responses_endpoint_from_api_base_removes_query_and_fragment() -> anyhow::Result<()> {
        let api_base = Url::parse("https://api.openai.com/v1?query=1#fragment")?;
        let endpoint = responses_endpoint_from_api_base(&api_base);
        assert_eq!(endpoint.as_str(), "https://api.openai.com/v1/responses");
        assert!(endpoint.query().is_none());
        assert!(endpoint.fragment().is_none());

        Ok(())
    }

    #[test]
    fn test_request_message_count_with_text_input() {
        let request = oai::CreateResponse {
            input: oai::InputParam::Text("test".to_string()),
            ..Default::default()
        };
        assert_eq!(request_message_count(&request), 1);
    }

    #[test]
    fn test_request_message_count_with_items_input() {
        let request = oai::CreateResponse {
            input: oai::InputParam::Items(vec![
                oai::InputItem::Item(oai::Item::FunctionCall(oai::FunctionToolCall {
                    id: Some("call_1".to_string()),
                    call_id: "call_id_1".to_string(),
                    name: "tool1".to_string(),
                    arguments: "args1".to_string(),
                    status: None,
                })),
                oai::InputItem::Item(oai::Item::FunctionCall(oai::FunctionToolCall {
                    id: Some("call_2".to_string()),
                    call_id: "call_id_2".to_string(),
                    name: "tool2".to_string(),
                    arguments: "args2".to_string(),
                    status: None,
                })),
            ]),
            ..Default::default()
        };
        assert_eq!(request_message_count(&request), 2);
    }

    #[test]
    fn test_request_message_count_with_empty_items() {
        let request =
            oai::CreateResponse { input: oai::InputParam::Items(vec![]), ..Default::default() };
        assert_eq!(request_message_count(&request), 0);
    }

    #[test]
    fn test_openai_responses_provider_new_with_api_key() {
        let provider = openai_responses("test-key", "https://api.openai.com/v1");
        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);

        assert_eq!(provider_impl.api_base.as_str(), "https://api.openai.com/v1");
        assert_eq!(
            provider_impl.responses_url.as_str(),
            "https://api.openai.com/v1/responses"
        );
    }

    #[test]
    fn test_openai_responses_provider_new_with_oauth_with_api_key() {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1").unwrap(),
            credential: Some(forge_domain::AuthCredential {
                id: ProviderId::OPENAI,
                auth_details: forge_domain::AuthDetails::OAuthWithApiKey {
                    tokens: forge_domain::OAuthTokens::new(
                        "access-token",
                        None::<String>,
                        chrono::Utc::now() + chrono::Duration::hours(1),
                    ),
                    api_key: forge_domain::ApiKey::from("oauth-key".to_string()),
                    config: forge_domain::OAuthConfig {
                        auth_url: Url::parse("https://example.com/auth").unwrap(),
                        token_url: Url::parse("https://example.com/token").unwrap(),
                        client_id: forge_domain::ClientId::from("client-id".to_string()),
                        scopes: vec![],
                        redirect_uri: None,
                        use_pkce: false,
                        token_refresh_url: None,
                        custom_headers: None,
                        extra_auth_params: None,
                    },
                },
                url_params: HashMap::new(),
            }),
            auth_methods: vec![],
            url_params: vec![],
            models: None,
        };

        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);
        assert_eq!(provider_impl.api_base.as_str(), "https://api.openai.com/v1");
    }

    #[test]
    fn test_openai_responses_provider_new_with_oauth() {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1").unwrap(),
            credential: Some(forge_domain::AuthCredential {
                id: ProviderId::OPENAI,
                auth_details: forge_domain::AuthDetails::OAuth {
                    tokens: forge_domain::OAuthTokens::new(
                        "access-token",
                        None::<String>,
                        chrono::Utc::now() + chrono::Duration::hours(1),
                    ),
                    config: forge_domain::OAuthConfig {
                        auth_url: Url::parse("https://example.com/auth").unwrap(),
                        token_url: Url::parse("https://example.com/token").unwrap(),
                        client_id: forge_domain::ClientId::from("client-id".to_string()),
                        scopes: vec![],
                        redirect_uri: None,
                        use_pkce: false,
                        token_refresh_url: None,
                        custom_headers: None,
                        extra_auth_params: None,
                    },
                },
                url_params: HashMap::new(),
            }),
            auth_methods: vec![],
            url_params: vec![],
            models: None,
        };

        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);
        assert_eq!(provider_impl.api_base.as_str(), "https://api.openai.com/v1");
    }

    #[test]
    fn test_openai_responses_provider_new_without_credential() {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1").unwrap(),
            credential: None,
            auth_methods: vec![],
            url_params: vec![],
            models: None,
        };

        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);
        assert_eq!(provider_impl.api_base.as_str(), "https://api.openai.com/v1");
    }

    #[test]
    fn test_get_headers_with_api_key() {
        let provider = openai_responses("test-key", "https://api.openai.com/v1");
        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);

        let headers = provider_impl.get_headers();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, "authorization");
        assert_eq!(headers[0].1, "Bearer test-key");
    }

    #[test]
    fn test_get_headers_with_oauth_device_custom_headers() {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1").unwrap(),
            credential: make_credential(ProviderId::OPENAI, "test-key"),
            auth_methods: vec![forge_domain::AuthMethod::OAuthDevice(
                forge_domain::OAuthConfig {
                    auth_url: Url::parse("https://example.com/auth").unwrap(),
                    token_url: Url::parse("https://example.com/token").unwrap(),
                    client_id: forge_domain::ClientId::from("client-id".to_string()),
                    scopes: vec![],
                    redirect_uri: None,
                    use_pkce: false,
                    token_refresh_url: None,
                    custom_headers: Some(
                        [("X-Custom".to_string(), "value".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    extra_auth_params: None,
                },
            )],
            url_params: vec![],
            models: None,
        };

        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);
        let headers = provider_impl.get_headers();

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0, "authorization");
        assert_eq!(headers[1].0, "X-Custom");
        assert_eq!(headers[1].1, "value");
    }

    #[test]
    fn test_get_headers_with_oauth_code_custom_headers() {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1").unwrap(),
            credential: make_credential(ProviderId::OPENAI, "test-key"),
            auth_methods: vec![forge_domain::AuthMethod::OAuthCode(
                forge_domain::OAuthConfig {
                    auth_url: Url::parse("https://example.com/auth").unwrap(),
                    token_url: Url::parse("https://example.com/token").unwrap(),
                    client_id: forge_domain::ClientId::from("client-id".to_string()),
                    scopes: vec![],
                    redirect_uri: None,
                    use_pkce: false,
                    token_refresh_url: None,
                    custom_headers: Some(
                        [("X-Custom".to_string(), "value".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    extra_auth_params: None,
                },
            )],
            url_params: vec![],
            models: None,
        };

        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);
        let headers = provider_impl.get_headers();

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0, "authorization");
        assert_eq!(headers[1].0, "X-Custom");
        assert_eq!(headers[1].1, "value");
    }

    #[test]
    fn test_get_headers_without_credential() {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1").unwrap(),
            credential: None,
            auth_methods: vec![],
            url_params: vec![],
            models: None,
        };

        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);
        let headers = provider_impl.get_headers();

        assert!(headers.is_empty());
    }

    #[test]
    fn test_get_headers_with_multiple_custom_headers() {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1").unwrap(),
            credential: make_credential(ProviderId::OPENAI, "test-key"),
            auth_methods: vec![forge_domain::AuthMethod::OAuthDevice(
                forge_domain::OAuthConfig {
                    auth_url: Url::parse("https://example.com/auth").unwrap(),
                    token_url: Url::parse("https://example.com/token").unwrap(),
                    client_id: forge_domain::ClientId::from("client-id".to_string()),
                    scopes: vec![],
                    redirect_uri: None,
                    use_pkce: false,
                    token_refresh_url: None,
                    custom_headers: Some(
                        [
                            ("X-Header1".to_string(), "value1".to_string()),
                            ("X-Header2".to_string(), "value2".to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    extra_auth_params: None,
                },
            )],
            url_params: vec![],
            models: None,
        };

        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl = OpenAIResponsesProvider::<MockHttpClient>::new(provider, infra);
        let headers = provider_impl.get_headers();

        assert_eq!(headers.len(), 3);
        let header_names: Vec<&str> = headers.iter().map(|h| h.0.as_str()).collect();
        assert!(header_names.contains(&"authorization"));
        assert!(header_names.contains(&"X-Header1"));
        assert!(header_names.contains(&"X-Header2"));
    }

    #[test]
    fn test_openai_responses_repository_new() {
        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let repo = OpenAIResponsesResponseRepository::new(infra.clone());

        assert_eq!(
            repo.retry_config.retry_status_codes,
            vec![429, 500, 502, 503, 504, 408]
        );
    }

    #[tokio::test]
    async fn test_openai_responses_repository_models_returns_empty() -> anyhow::Result<()> {
        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let repo = OpenAIResponsesResponseRepository::new(infra);

        let provider = openai_responses("test-key", "https://api.openai.com/v1");
        let models = repo.models(provider).await?;

        assert!(models.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_openai_responses_provider_uses_direct_http_calls() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;

        // Create SSE events for streaming response
        let events = vec![
            "event: response.output_text.delta".to_string(),
            format!(
                "data: {}",
                serde_json::json!({
                    "type": "response.output_text.delta",
                    "sequence_number": 1,
                    "item_id": "item_1",
                    "output_index": 0,
                    "content_index": 0,
                    "delta": "hello"
                })
            ),
            "event: response.completed".to_string(),
            format!(
                "data: {}",
                serde_json::json!({
                    "type": "response.completed",
                    "sequence_number": 2,
                    "response": openai_response_fixture()
                })
            ),
            "event: done".to_string(),
            "data: [DONE]".to_string(),
        ];

        let mock = fixture.mock_responses_stream(events, 200).await;

        let provider = openai_responses(
            "test-api-key",
            &format!("{}/v1/chat/completions", fixture.url()),
        );

        let infra = Arc::new(MockHttpClient { client: reqwest::Client::new() });
        let provider_impl: OpenAIResponsesProvider<_> =
            OpenAIResponsesProvider::new(provider, infra);
        let context = ChatContext::default()
            .add_message(ContextMessage::user("Hi", None))
            .stream(true);

        let mut stream = provider_impl
            .chat(&ModelId::from("codex-mini-latest"), context)
            .await?;

        let first = stream.next().await.expect("stream should yield")?;

        mock.assert_async().await;
        assert_eq!(first.content, Some(Content::part("hello")));

        let second = stream
            .next()
            .await
            .expect("stream should yield second message")?;
        assert_eq!(second.finish_reason, Some(FinishReason::Stop));

        Ok(())
    }
}
