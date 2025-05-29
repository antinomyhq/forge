use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, EnvironmentService, Model, ModelId,
    ProviderService, ResultStream,
};
use forge_provider::Client;

use crate::Infrastructure;

#[derive(Clone)]
pub struct ForgeProviderService {
    // The provider service implementation
    client: Arc<Client>,
}

impl ForgeProviderService {
    pub fn new<F: Infrastructure>(infra: Arc<F>) -> Self {
        let infra = infra.clone();
        let env = infra.environment_service().get_environment();
        let provider = env.provider.clone();
        let version = env.version();
        Self {
            client: Arc::new(
                Client::new(provider, env.retry_config.retry_status_codes, version).unwrap(),
            ),
        }
    }
}

#[async_trait::async_trait]
impl ProviderService for ForgeProviderService {
    async fn chat(
        &self,
        model: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        self.client.models().await
    }

    async fn model(&self, model: &ModelId) -> Result<Option<Model>> {
        self.client.model(model).await
    }
}
