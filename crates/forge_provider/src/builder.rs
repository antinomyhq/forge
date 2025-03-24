// Context trait is needed for error handling in the provider implementations

use anyhow::{Context as _, Result};
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ProviderService, ResultStream,
};

use crate::anthropic::Anthropic;
use crate::open_router::OpenRouter;

pub enum Client {
    OpenAICompat(OpenRouter),
    Anthropic(Anthropic),
}

impl Client {
    pub fn new(provider: Provider) -> Result<Self> {
        let client = reqwest::Client::builder().build()?;

        match &provider {
            Provider::OpenAI { url, .. } => Ok(Client::OpenAICompat(
                OpenRouter::builder()
                    .client(client)
                    .provider(provider.clone())
                    .build()
                    .with_context(|| format!("Failed to initialize: {}", url))?,
            )),

            Provider::Anthropic { url, key } => Ok(Client::Anthropic(
                Anthropic::builder()
                    .client(client)
                    .api_key(key.to_string())
                    .base_url(url.clone())
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
