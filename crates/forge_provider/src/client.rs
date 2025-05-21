// Context trait is needed for error handling in the provider implementations

use std::sync::Arc;

use anyhow::{Context as _, Result};
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ProviderService, ResultStream,
};
use reqwest::redirect::Policy;
use tokio_stream::StreamExt;

use crate::anthropic::Anthropic;
use crate::forge_provider::ForgeProvider;
use crate::retry::into_retry;

#[derive(Clone)]
pub struct Client {
    retry_status_codes: Arc<Vec<u16>>,
    inner: Arc<InnerClient>,
}

enum InnerClient {
    OpenAICompat(ForgeProvider),
    Anthropic(Anthropic),
}

impl Client {
    pub fn new(provider: Provider, retry_status_codes: Vec<u16>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .read_timeout(std::time::Duration::from_secs(60))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(5)
            .redirect(Policy::limited(10))
            .build()?;

        let inner = match &provider {
            Provider::OpenAI { url, .. } => InnerClient::OpenAICompat(
                ForgeProvider::builder()
                    .client(client)
                    .provider(provider.clone())
                    .build()
                    .with_context(|| format!("Failed to initialize: {url}"))?,
            ),

            Provider::Anthropic { url, key } => InnerClient::Anthropic(
                Anthropic::builder()
                    .client(client)
                    .api_key(key.to_string())
                    .base_url(url.clone())
                    .anthropic_version("2023-06-01".to_string())
                    .build()
                    .with_context(|| {
                        format!("Failed to initialize Anthropic client with URL: {url}")
                    })?,
            ),
        };
        Ok(Self {
            inner: Arc::new(inner),
            retry_status_codes: Arc::new(retry_status_codes),
        })
    }

    fn retry<A>(&self, result: anyhow::Result<A>) -> anyhow::Result<A> {
        let codes = &self.retry_status_codes;
        result.map_err(move |e| into_retry(e, codes))
    }
}

#[async_trait::async_trait]
impl ProviderService for Client {
    async fn chat(
        &self,
        model: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let chat_stream = self.clone().retry(match self.inner.as_ref() {
            InnerClient::OpenAICompat(provider) => provider.chat(model, context).await,
            InnerClient::Anthropic(provider) => provider.chat(model, context).await,
        })?;

        let this = self.clone();
        Ok(Box::pin(
            chat_stream.map(move |item| this.clone().retry(item)),
        ))
    }

    async fn models(&self) -> anyhow::Result<Vec<Model>> {
        self.clone().retry(match self.inner.as_ref() {
            InnerClient::OpenAICompat(provider) => provider.models().await,
            InnerClient::Anthropic(provider) => provider.models().await,
        })
    }
}
