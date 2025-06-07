use std::sync::Arc;

use anyhow::Result;
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, Model, ModelId, ResultStream, RetryConfig,
};
use forge_provider::Client;

use crate::services::{EnvironmentService, ProviderService};
use crate::Infrastructure;

#[derive(Clone)]
pub struct ForgeProviderService {
    // The provider service implementation
    client: Arc<Client>,
    retry_config: RetryConfig,
}

impl ForgeProviderService {
    pub fn new<F: Infrastructure>(infra: Arc<F>) -> Self {
        let infra = infra.clone();
        let env = infra.environment_service().get_environment();
        let provider = env.provider.clone();
        let retry_config = env.retry_config.clone();
        let version = env.version();
        Self {
            client: Arc::new(Client::new(provider, retry_config.clone(), version).unwrap()),
            retry_config,
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
        self.retry_config
            .retry(|| self.client.chat(model, request.clone()))
            .await
    }

    async fn models(&self) -> Result<Vec<Model>> {
        self.retry_config.retry(|| self.client.models()).await
    }
}
