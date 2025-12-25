use crate::provider_client::BedrockProvider;
use crate::provider_client::into_retry;
use anyhow::Context as _;
use derive_setters::Setters;
use forge_app::domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ResultStream, RetryConfig,
};
use forge_domain::ChatRepository;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio_stream::StreamExt;
use url::Url;

/// Repository for AWS Bedrock provider responses
#[derive(Setters)]
#[setters(strip_option, into)]
pub struct BedrockResponseRepository<F> {
    retry_config: Arc<RetryConfig>,
    _phantom: PhantomData<F>,
}

impl<F> BedrockResponseRepository<F> {
    pub fn new() -> Self {
        Self {
            retry_config: Arc::new(RetryConfig::default()),
            _phantom: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<F: forge_app::HttpInfra + Send + Sync + 'static> ChatRepository
    for BedrockResponseRepository<F>
{
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let retry_config = self.retry_config.clone();
        let provider_client: BedrockProvider<F> =
            BedrockProvider::new(provider).map_err(|e| into_retry(e, &retry_config))?;

        let stream = provider_client
            .chat(model_id, context)
            .await
            .map_err(|e| into_retry(e, &retry_config))?;

        Ok(Box::pin(stream.map(move |item| {
            item.map_err(|e| into_retry(e, &retry_config))
        })))
    }

    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        let retry_config = self.retry_config.clone();
        let provider_client: BedrockProvider<F> = BedrockProvider::new(provider)?;
        provider_client
            .models()
            .await
            .map_err(|e| into_retry(e, &retry_config))
            .context("Failed to fetch models from Bedrock provider")
    }
}
