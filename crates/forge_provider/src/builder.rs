// Context trait is needed for error handling in the provider implementations

use anyhow::{Context as _, Result};
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ProviderService, ResultStream,
    RetryConfig,
};
use reqwest::redirect::Policy;

use crate::anthropic::Anthropic;
use crate::forge_provider::ForgeProvider;

pub enum Client {
    OpenAICompat(ForgeProvider),
    Anthropic(Anthropic),
}

impl Client {
    pub fn new(provider: Provider, retry_config: RetryConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .read_timeout(std::time::Duration::from_secs(60))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(5)
            .redirect(Policy::limited(10))
            .build()?;

        match &provider {
            Provider::OpenAI { url, .. } => Ok(Client::OpenAICompat(
                ForgeProvider::builder()
                    .client(client)
                    .provider(provider.clone())
                    .retry_config(retry_config.clone())
                    .build()
                    .with_context(|| format!("Failed to initialize: {url}"))?,
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
