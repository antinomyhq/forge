// Context trait is needed for error handling in the provider implementations

use anyhow::{Context as _, Result};
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ProviderService, ResultStream,
    RetryConfig,
};

use std::path::PathBuf;

use crate::anthropic::Anthropic;
use crate::http_client::MockableHttpClient;
use crate::open_router::OpenRouter;

pub enum Client {
    OpenAICompat(OpenRouter),
    Anthropic(Anthropic),
}

impl Client {
    pub fn new(provider: Provider, retry_config: RetryConfig) -> Result<Self> {
        // Check if mock mode is enabled
        let mock_data_dir = std::env::var("FORGE_MOCK_DIR").ok().map(PathBuf::from);
        let record_mode = std::env::var("FORGE_MOCK_UPDATE")
            .map(|val| val.to_lowercase() == "true")
            .unwrap_or(false);
        let mock_enabled = std::env::var("FORGE_MOCK")
            .map(|val| val.to_lowercase() == "true")
            .unwrap_or(false);

        // Create the HTTP client
        let reqwest_client = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(5)
            .build()?;

        // Create the mockable HTTP client if mock mode is enabled
        let mockable_client = if mock_enabled {
            MockableHttpClient::new(
                reqwest_client.clone(),
                mock_data_dir,
                record_mode,
            )
        } else {
            // If mock mode is not enabled, create a pass-through client
            MockableHttpClient::new(
                reqwest_client.clone(),
                None,
                false,
            )
        };

        match &provider {
            Provider::OpenAI { url, .. } => Ok(Client::OpenAICompat(
                OpenRouter::builder()
                    .client(mockable_client)
                    .provider(provider.clone())
                    .retry_config(retry_config.clone())
                    .build()
                    .with_context(|| format!("Failed to initialize: {url}"))?,
            )),

            Provider::Anthropic { url, key } => Ok(Client::Anthropic(
                Anthropic::builder()
                    .client(mockable_client)
                    .api_key(key.to_string())
                    .base_url(url.clone())
                    .anthropic_version("2023-06-01".to_string())
                    .retry_config(retry_config.clone())
                    .build()
                    .with_context(|| {
                        format!("Failed to initialize Anthropic client with URL: {url}")
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
