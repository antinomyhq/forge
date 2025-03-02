use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, Model, ModelId, Parameters, ProviderService,
    ResultStream,
};
use forge_open_router::ProviderBuilder;
use moka2::future::Cache;

use crate::{EnvironmentService, Infrastructure};

pub struct ForgeProviderService {
    or: Box<dyn ProviderService>,
    cache: Cache<ModelId, Parameters>,
}

impl ForgeProviderService {
    pub fn new<F: Infrastructure>(infra: Arc<F>) -> Self {
        let env = infra.environment_service().get_environment();
        let or = ProviderBuilder::from_url(env.provider_url)
            .with_key(env.provider_key)
            .build()
            .expect("Failed to build provider");

        Self { or, cache: Cache::new(1024) }
    }
}

#[async_trait::async_trait]
impl ProviderService for ForgeProviderService {
    async fn chat(
        &self,
        model_id: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.or
            .chat(model_id, request)
            .await
            .with_context(|| format!("Failed to chat with model: {}", model_id))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        self.or.models().await
    }

    async fn parameters(&self, model: &ModelId) -> anyhow::Result<Parameters> {
        Ok(self
            .cache
            .try_get_with_by_ref(model, async {
                self.or
                    .parameters(model)
                    .await
                    .with_context(|| format!("Failed to get parameters for model: {}", model))
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))?)
    }
}
