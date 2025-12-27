use std::sync::Arc;

use anyhow::Context as _;
use async_openai::Client as AsyncOpenAIClient;
use async_openai::config::OpenAIConfig;
use async_openai::traits::RequestOptionsBuilder as _;
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

use crate::provider::retry::into_retry;
use crate::provider::utils::{create_headers, format_http_context, sanitize_headers};
use crate::provider::{FromDomain, IntoDomain};

#[derive(Clone)]
pub(super) struct OpenAIResponsesProvider<H> {
    provider: Provider<Url>,
    client: Arc<AsyncOpenAIClient<OpenAIConfig>>,
    api_base: Url,
    responses_url: Url,
    _phantom: std::marker::PhantomData<H>,
}

impl<H: HttpInfra> OpenAIResponsesProvider<H> {
    /// Creates a new OpenAI Responses provider
    ///
    /// # Panics
    ///
    /// Panics if the provider URL cannot be converted to an API base URL
    pub fn new(provider: Provider<Url>) -> Self {
        let api_base = api_base_from_endpoint_url(&provider.url)
            .expect("Failed to derive API base URL from provider endpoint");
        let responses_url = responses_endpoint_from_api_base(&api_base);

        let api_key = provider
            .credential
            .as_ref()
            .map(|c| match &c.auth_details {
                forge_domain::AuthDetails::ApiKey(key) => key.as_str(),
                forge_domain::AuthDetails::OAuthWithApiKey { api_key, .. } => api_key.as_str(),
                forge_domain::AuthDetails::OAuth { tokens, .. } => tokens.access_token.as_str(),
            })
            .unwrap_or("");

        let config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(api_base.as_str());

        let client = Arc::new(AsyncOpenAIClient::with_config(config));

        Self {
            provider,
            client,
            api_base,
            responses_url,
            _phantom: std::marker::PhantomData,
        }
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

        self.client
            .responses()
            .headers(headers)
            .create_stream(request)
            .await
            .with_context(|| format_http_context(None, "POST", &self.responses_url))?
            .into_domain()
    }
}

/// Derives an API base URL suitable for `async-openai` from a configured
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
    #[allow(dead_code)]
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
        let provider_client: OpenAIResponsesProvider<F> = OpenAIResponsesProvider::new(provider);
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
            _url: &reqwest::Url,
            _headers: Option<reqwest::header::HeaderMap>,
            _body: bytes::Bytes,
        ) -> anyhow::Result<reqwest_eventsource::EventSource> {
            unimplemented!()
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

    #[tokio::test]
    async fn test_openai_responses_provider_uses_responses_api_via_async_openai()
    -> anyhow::Result<()> {
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

        let provider: OpenAIResponsesProvider<MockHttpClient> =
            OpenAIResponsesProvider::new(provider);
        let context = ChatContext::default()
            .add_message(ContextMessage::user("Hi", None))
            .stream(true);

        let mut stream = provider
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
