// Context trait is needed for error handling in the provider implementations

use anyhow::{Context as _, Result};
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ProviderService, ResultStream,
    RetryConfig,
};
use reqwest::Client as ReqwestClient;
use std::path::PathBuf;

use crate::anthropic::Anthropic;
use crate::mock_client::{MockClient, MockClientConfig, MockMode};
use crate::open_router::OpenRouter;

pub enum Client {
    OpenAICompat(OpenRouter),
    Anthropic(Anthropic),
}

impl Client {
    pub fn new(provider: Provider, retry_config: RetryConfig) -> Result<Self> {
        let client = ReqwestClient::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(5)
            .build()?;

        Self::with_client(provider, retry_config, client)
    }

    /// Create a new client with a mock HTTP client for testing
    pub fn with_mock(
        provider: Provider, 
        retry_config: RetryConfig, 
        mock_mode: MockMode,
        cache_dir: Option<PathBuf>,
        update_cache: bool,
    ) -> Result<Self> {
        let config = MockClientConfig {
            mode: mock_mode,
            cache_dir: cache_dir.unwrap_or_else(|| PathBuf::from("tests/fixtures/http_cache")),
            update_cache,
        };
        
        let client = MockClient::new(config);
        Self::with_client(provider, retry_config, client)
    }

    /// Create a new client with the given HTTP client
    fn with_client<C>(provider: Provider, retry_config: RetryConfig, client: C) -> Result<Self> 
    where
        C: Into<reqwest::Client> + Clone + Send + Sync + 'static,
    {
        let client = client.into();
        
        match &provider {
            Provider::OpenAI { url, .. } => Ok(Client::OpenAICompat(
                OpenRouter::builder()
                    .client(client)
                    .provider(provider.clone())
                    .retry_config(retry_config.clone())
                    .build()
                    .with_context(|| format!("Failed to initialize: {}", url))?,
            )),

            Provider::Anthropic { url, key } => Ok(Client::Anthropic(
                Anthropic::builder()
                    .client(client)
                    .api_key(key.to_string())
                    .base_url(url.clone())
                    .anthropic_version("2023-06-01".to_string())
                    .retry_config(retry_config.clone())
                    .build()
                    .with_context(|| {
                        format!("Failed to initialize Anthropic client with URL: {}", url)
                    })?,
            )),
        }
    }
}

#[async_trait::async_trait]
impl ProviderService for Client {
    async fn chat(
        &self,
        model: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        match self {
            Client::OpenAICompat(provider) => provider.chat(model, context).await,
            Client::Anthropic(provider) => provider.chat(model, context).await,
        }
    }

    async fn models(&self) -> anyhow::Result<Vec<Model>> {
        match self {
            Client::OpenAICompat(provider) => provider.models().await,
            Client::Anthropic(provider) => provider.models().await,
        }
    }
}
