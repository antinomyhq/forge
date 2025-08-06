// Context trait is needed for error handling in the provider implementations

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use derive_setters::Setters;
use forge_app::HttpClientService;
use forge_app::domain::{
    ChatCompletionMessage, Context, HttpConfig, Model, ModelId, Provider, ResultStream, RetryConfig,
};
use reqwest::Url;
use reqwest::header::HeaderMap;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;

use crate::provider::anthropic::Anthropic;
use crate::provider::openai::OpenAIProvider;
use crate::provider::retry::into_retry;

#[derive(Setters)]
#[setters(strip_option, into)]
pub struct ClientBuilder {
    pub retry_config: Arc<RetryConfig>,
    pub timeout_config: HttpConfig,
    pub use_hickory: bool,
    pub provider: Provider,
    #[allow(dead_code)]
    pub version: String,
}

impl ClientBuilder {
    /// Create a new ClientBuilder with required provider and version
    /// parameters.
    pub fn new(provider: Provider, version: impl Into<String>) -> Self {
        Self {
            retry_config: Arc::new(RetryConfig::default()),
            timeout_config: HttpConfig::default(),
            use_hickory: false,
            provider,
            version: version.into(),
        }
    }

    /// Build the client with the configured settings.
    pub fn build<T: HttpClientService>(self, http: Arc<T>) -> Result<Client<T>> {
        let provider = self.provider;
        let retry_config = self.retry_config;

        let inner = match &provider {
            Provider::OpenAI { .. } => {
                InnerClient::OpenAICompat(OpenAIProvider::new(provider.clone(), http.clone()))
            }

            Provider::Anthropic { key, .. } => InnerClient::Anthropic(Anthropic::new(
                http.clone(),
                key.to_string(),
                provider.to_base_url().to_string(),
                "2023-06-01".to_string(),
            )),
        };

        Ok(Client {
            inner: Arc::new(inner),
            retry_config,
            models_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

pub struct Client<T> {
    retry_config: Arc<RetryConfig>,
    inner: Arc<InnerClient<T>>,
    models_cache: Arc<RwLock<HashMap<ModelId, Model>>>,
}

impl<T> Clone for Client<T> {
    fn clone(&self) -> Self {
        Self {
            retry_config: self.retry_config.clone(),
            inner: self.inner.clone(),
            models_cache: self.models_cache.clone(),
        }
    }
}

enum InnerClient<T> {
    OpenAICompat(OpenAIProvider<T>),
    Anthropic(Anthropic<T>),
}

impl<T: HttpClientService> Client<T> {
    fn retry<A>(&self, result: anyhow::Result<A>) -> anyhow::Result<A> {
        let retry_config = &self.retry_config;
        result.map_err(move |e| into_retry(e, retry_config))
    }

    pub async fn refresh_models(&self) -> anyhow::Result<Vec<Model>> {
        let models = self.clone().retry(match self.inner.as_ref() {
            InnerClient::OpenAICompat(provider) => provider.models().await,
            InnerClient::Anthropic(provider) => provider.models().await,
        })?;

        // Update the cache with all fetched models
        {
            let mut cache = self.models_cache.write().await;
            cache.clear(); // Clear existing cache to ensure freshness
            for model in &models {
                cache.insert(model.id.clone(), model.clone());
            }
        }

        Ok(models)
    }
}

impl<T: HttpClientService> Client<T> {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let chat_stream = self.clone().retry(match self.inner.as_ref() {
            InnerClient::OpenAICompat(provider) => provider.chat(model, context).await,
            InnerClient::Anthropic(provider) => provider.chat(model, context).await,
        })?;

        let this: Client<T> = self.clone();
        Ok(Box::pin(
            chat_stream.map(move |item| this.clone().retry(item)),
        ))
    }

    pub async fn models(&self) -> anyhow::Result<Vec<Model>> {
        self.refresh_models().await
    }

    #[allow(dead_code)]
    pub async fn model(&self, model: &ModelId) -> anyhow::Result<Option<Model>> {
        // First, check if the model is in the cache
        {
            let cache = self.models_cache.read().await;
            if let Some(model) = cache.get(model) {
                return Ok(Some(model.clone()));
            }
        }

        // Cache miss - refresh models (which will populate the cache) and find the
        // model in the result
        let models = self.refresh_models().await?;
        Ok(models.into_iter().find(|m| m.id == *model))
    }
}

pub fn join_url(base_url: &str, path: &str) -> anyhow::Result<Url> {
    // Validate the path doesn't contain certain patterns
    if path.contains("://") || path.contains("..") {
        anyhow::bail!("Invalid path: Contains forbidden patterns");
    }

    // Remove leading slash to avoid double slashes
    let path = path.trim_start_matches('/');

    let url = Url::parse(base_url)
        .with_context(|| format!("Failed to parse base URL: {base_url}"))?
        .join(path)
        .with_context(|| format!("Failed to append {path} to base URL: {base_url}"))?;
    Ok(url)
}

pub fn create_headers(headers: Vec<(String, String)>) -> HeaderMap {
    let mut header_map = HeaderMap::new();
    for (key, value) in headers {
        let header_name =
            reqwest::header::HeaderName::from_bytes(key.as_bytes()).expect("Invalid header name");
        let header_value = value.parse().expect("Invalid header value");
        header_map.insert(header_name, header_value);
    }
    header_map
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use forge_app::HttpClientService;
    use forge_app::domain::Provider;
    use reqwest::Url;
    use reqwest::header::HeaderMap;
    use reqwest_eventsource::EventSource;

    use super::*;

    // Simple mock for testing client functionality
    struct MockHttpClient;

    #[async_trait::async_trait]
    impl HttpClientService for MockHttpClient {
        async fn get(
            &self,
            _url: &Url,
            _headers: Option<HeaderMap>,
        ) -> anyhow::Result<reqwest::Response> {
            Err(anyhow::anyhow!("Mock HTTP client - no real requests"))
        }

        async fn post(&self, _url: &Url, _body: Bytes) -> anyhow::Result<reqwest::Response> {
            Err(anyhow::anyhow!("Mock HTTP client - no real requests"))
        }

        async fn delete(&self, _url: &Url) -> anyhow::Result<reqwest::Response> {
            Err(anyhow::anyhow!("Mock HTTP client - no real requests"))
        }

        async fn eventsource(
            &self,
            _url: &Url,
            _headers: Option<HeaderMap>,
            _body: Bytes,
        ) -> anyhow::Result<EventSource> {
            Err(anyhow::anyhow!("Mock HTTP client - no real requests"))
        }
    }

    #[tokio::test]
    async fn test_cache_initialization() {
        let provider = Provider::OpenAI {
            url: Url::parse("https://api.openai.com/v1/").unwrap(),
            key: Some("test-key".to_string()),
        };
        let client = ClientBuilder::new(provider, "dev")
            .build(Arc::new(MockHttpClient))
            .unwrap();

        // Verify cache is initialized as empty
        let cache = client.models_cache.read().await;
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_refresh_models_method_exists() {
        let provider = Provider::OpenAI {
            url: Url::parse("https://api.openai.com/v1/").unwrap(),
            key: Some("test-key".to_string()),
        };
        let client = ClientBuilder::new(provider, "dev")
            .build(Arc::new(MockHttpClient))
            .unwrap();

        // Verify refresh_models method is available (it will fail due to no actual API,
        // but that's expected)
        let result = client.refresh_models().await;
        assert!(result.is_err()); // Expected to fail since we're not hitting a
        // real API
    }

    #[tokio::test]
    async fn test_builder_pattern_api() {
        let provider = Provider::OpenAI {
            url: Url::parse("https://api.openai.com/v1/").unwrap(),
            key: Some("test-key".to_string()),
        };

        // Test the builder pattern API
        let client = ClientBuilder::new(provider, "dev")
            .retry_config(Arc::new(RetryConfig::default()))
            .timeout_config(HttpConfig::default())
            .use_hickory(true)
            .build(Arc::new(MockHttpClient))
            .unwrap();

        // Verify cache is initialized as empty
        let cache = client.models_cache.read().await;
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_builder_with_defaults() {
        let provider = Provider::OpenAI {
            url: Url::parse("https://api.openai.com/v1/").unwrap(),
            key: Some("test-key".to_string()),
        };

        // Test that ClientBuilder::new works with minimal parameters
        let client = ClientBuilder::new(provider, "dev")
            .build(Arc::new(MockHttpClient))
            .unwrap();

        // Verify cache is initialized as empty
        let cache = client.models_cache.read().await;
        assert!(cache.is_empty());
    }
}
