// Context trait is needed for error handling in the provider implementations

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use forge_domain::{
    ChatCompletionMessage, Context, HttpConfig, Model, ModelId, Provider, ResultStream, RetryConfig,
    HttpInfra,
};
use reqwest::Url;
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
#[derive(Clone)]
enum InnerClient {
    OpenAICompat(ForgeProvider),
    Anthropic(Anthropic),
}

impl Client {
    pub fn new(
        provider: Provider,
        retry_config: Arc<RetryConfig>,
        version: impl ToString,
        _timeout_config: &HttpConfig,
        http_infra: Arc<dyn HttpInfra>,
    ) -> Result<Self> {
        let inner = match &provider {
            Provider::OpenAI(details) => InnerClient::OpenAICompat(
                ForgeProvider::builder()
                    .http(http_infra.clone())
                    .provider(provider.clone())
                    .version(version.to_string())
                    .build()
                    .with_context(|| format!("Failed to initialize: {}", details.base_url))?,
            ),

            Provider::Anthropic(details) => InnerClient::Anthropic(
                Anthropic::builder()
                    .http(http_infra.clone())
                    .api_key(details.api_key.clone())
                    .base_url(Url::parse(&details.base_url)?)
                    .anthropic_version("2023-06-01".to_string())
                    .build()
                    .with_context(|| {
                        format!("Failed to initialize Anthropic client with URL: {}", details.base_url)
                    })?,
            ),
        };

        Ok(Self {
            inner: Arc::new(inner),
            retry_config,
            models_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn update_provider(
        &mut self, provider: Provider,
    ){
        match self.inner.as_ref() {
            InnerClient::OpenAICompat(inner) => {
                let mut new_inner = inner.clone();
                new_inner.update_provider(&provider);
                self.inner = Arc::new(InnerClient::OpenAICompat(new_inner));
            }
            InnerClient::Anthropic(inner) => {
                let mut new_inner = inner.clone();
                new_inner.update_provider(provider.api_key().to_string(), Url::parse(&provider.base_url()).unwrap(), "2023-06-01".to_string());
                self.inner = Arc::new(InnerClient::Anthropic(new_inner));
            }
        }
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
    use forge_domain::{Provider, ProviderDetails};
    use forge_infra::ForgeInfra;
    use super::*;

    #[tokio::test]
    async fn test_cache_initialization() {
        let provider = Provider::OpenAI(ProviderDetails::new(
            "test-openai".to_string(),
            "Test OpenAI".to_string(),
            "Test OpenAI provider".to_string(),
            "test-key".to_string(),
            "openai".to_string(),
            "https://api.openai.com/v1/".to_string(),
        ));
        let client = Client::new(
            provider,
            Arc::new(RetryConfig::default()),
            "dev",
            &HttpConfig::default(),
            Arc::new(ForgeInfra::new(false)),
        )
        .unwrap();

        // Verify cache is initialized as empty
        let cache = client.models_cache.read().await;
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_refresh_models_method_exists() {
        let provider = Provider::OpenAI(ProviderDetails::new(
            "test-openai".to_string(),
            "Test OpenAI".to_string(),
            "Test OpenAI provider".to_string(),
            "test-key".to_string(),
            "openai".to_string(),
            "https://api.openai.com/v1/".to_string(),
        ));
        let client = Client::new(
            provider,
            Arc::new(RetryConfig::default()),
            "dev",
            &HttpConfig::default(),
            Arc::new(ForgeInfra::new(false)),
        )
        .unwrap();

        // Verify refresh_models method is available (it will fail due to no actual API,
        // but that's expected)
        let result = client.refresh_models().await;
        assert!(result.is_err()); // Expected to fail since we're not hitting a
                                  // real API
    }
}
