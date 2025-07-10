// Context trait is needed for error handling in the provider implementations

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use forge_domain::{
    ChatCompletionMessage, Context, HttpConfig, Model, ModelId, Provider, ResultStream, RetryConfig,
};
use reqwest::redirect::Policy;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;

use crate::anthropic::Anthropic;
use crate::forge_provider::ForgeProvider;
use crate::retry::into_retry;

#[derive(Clone)]
pub struct Client {
    retry_config: Arc<RetryConfig>,
    inner: Arc<InnerClient>,
    models_cache: Arc<RwLock<HashMap<ModelId, Model>>>,
}

enum InnerClient {
    OpenAICompat(ForgeProvider),
    Anthropic(Anthropic),
}

impl Client {
    pub fn new(
        provider: Provider,
        retry_config: Arc<RetryConfig>,
        version: impl ToString,
        timeout_config: &HttpConfig,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(
                timeout_config.connect_timeout,
            ))
            .read_timeout(std::time::Duration::from_secs(timeout_config.read_timeout))
            .pool_idle_timeout(std::time::Duration::from_secs(
                timeout_config.pool_idle_timeout,
            ))
            .pool_max_idle_per_host(timeout_config.pool_max_idle_per_host)
            .redirect(Policy::limited(timeout_config.max_redirects))
            .build()?;

        let inner = match &provider {
            Provider::OpenAI { url, .. } => InnerClient::OpenAICompat(
                ForgeProvider::builder()
                    .client(client)
                    .provider(provider.clone())
                    .version(version.to_string())
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
            retry_config,
            models_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

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

impl Client {
    pub async fn chat(
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

    pub async fn models(&self) -> anyhow::Result<Vec<Model>> {
        self.refresh_models().await
    }

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

#[cfg(test)]
mod tests {
    use forge_domain::Provider;
    use reqwest::Url;

    use super::*;

    #[tokio::test]
    async fn test_cache_initialization() {
        let provider = Provider::OpenAI {
            url: Url::parse("https://api.openai.com/v1/").unwrap(),
            key: Some("test-key".to_string()),
        };
        let client = Client::new(
            provider,
            Arc::new(RetryConfig::default()),
            "dev",
            &HttpConfig::default(),
        )
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
        let client = Client::new(
            provider,
            Arc::new(RetryConfig::default()),
            "dev",
            &HttpConfig::default(),
        )
        .unwrap();

        // Verify refresh_models method is available (it will fail due to no actual API,
        // but that's expected)
        let result = client.refresh_models().await;
        assert!(result.is_err()); // Expected to fail since we're not hitting a
                                  // real API
    }
}
