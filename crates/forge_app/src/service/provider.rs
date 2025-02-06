use anyhow::{Context, Result};
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, Model, ModelId, Parameters, ProviderKind,
    ProviderService, ResultStream,
};
use forge_open_router::OpenRouterClient;
use moka2::future::Cache;

use super::Service;

impl Service {
    pub fn provider_service(
        api_key: Option<impl ToString>,
        base_url: Option<String>,
        provider: ProviderKind,
    ) -> impl ProviderService {
        Live::new(api_key, base_url, provider)
    }
}

struct Live {
    provider: Box<dyn ProviderService>,
    cache: Cache<ModelId, Parameters>,
}

impl Live {
    fn new(
        api_key: Option<impl ToString>,
        base_url: Option<String>,
        provider: ProviderKind,
    ) -> Self {
        let provider = OpenRouterClient::builder()
            .api_key(api_key.map(|x| x.to_string()))
            .base_url(base_url)
            .build(match provider {
                ProviderKind::Ollama => forge_open_router::Provider::Ollama(Default::default()),
                ProviderKind::OpenRouter => {
                    forge_open_router::Provider::OpenRouter(Default::default())
                }
            })
            .unwrap();

        Self { provider: Box::new(provider), cache: Cache::new(1024) }
    }
}

#[async_trait::async_trait]
impl ProviderService for Live {
    async fn chat(
        &self,
        model_id: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.provider.chat(model_id, request).await
    }

    async fn models(&self) -> Result<Vec<Model>> {
        self.provider.models().await
    }

    async fn parameters(&self, model: &ModelId) -> anyhow::Result<Parameters> {
        Ok(self
            .cache
            .try_get_with_by_ref(model, async {
                self.provider
                    .parameters(model)
                    .await
                    .with_context(|| format!("Failed to get parameters for model: {}", model))
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))?)
    }
}
