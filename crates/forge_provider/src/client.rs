// Context trait is needed for error handling in the provider implementations

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ProviderService, ResultStream,
};
use moka2::sync::Cache;
use reqwest::redirect::Policy;
use tokio_stream::StreamExt;

use crate::anthropic::Anthropic;
use crate::forge_provider::ForgeProvider;
use crate::retry::into_retry;

#[derive(Clone)]
pub struct Client {
    retry_status_codes: Arc<Vec<u16>>,
    inner: Arc<InnerClient>,
    cache: Arc<Cache<ModelId, Model>>,
}

enum InnerClient {
    OpenAICompat(ForgeProvider),
    Anthropic(Anthropic),
}

impl Client {
    pub fn new(provider: Provider, retry_status_codes: Vec<u16>, cache_ttl: u64) -> Result<Self> {
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

        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(cache_ttl))
            .build();

        Ok(Self {
            inner: Arc::new(inner),
            retry_status_codes: Arc::new(retry_status_codes),
            cache: Arc::new(cache),
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
        let models = self.clone().retry(match self.inner.as_ref() {
            InnerClient::OpenAICompat(provider) => provider.models().await,
            InnerClient::Anthropic(provider) => provider.models().await,
        })?;
        Ok(models)
    }

    async fn model(&self, model: &ModelId) -> anyhow::Result<Option<Model>> {
        // Try to get from cache first
        if let Some(cached_model) = self.cache.get(model) {
            return Ok(Some(cached_model));
        }

        // If not in cache, fetch from provider
        let model_result = self.clone().retry(match self.inner.as_ref() {
            InnerClient::OpenAICompat(provider) => provider.model(model).await,
            InnerClient::Anthropic(provider) => provider.model(model).await,
        })?;

        // If model exists, store in cache for future use
        if let Some(model_data) = &model_result {
            self.cache.insert(model.clone(), model_data.clone());
        }

        Ok(model_result)
    }
}
