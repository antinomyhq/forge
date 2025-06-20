use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::ProviderService;
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, ForgeKey, HttpConfig, Model, ModelId, Provider,
    ResultStream, RetryConfig,
};
use forge_provider::Client;
use tokio::sync::RwLock;

use crate::{EnvironmentInfra, ProviderInfra};

#[derive(Clone)]
pub struct ForgeProviderService<I> {
    infra: Arc<I>,
    retry_config: Arc<RetryConfig>,
    cached_client: Arc<RwLock<Option<Client>>>,
    version: String,
    timeout_config: HttpConfig,
}

impl<I: ProviderInfra + EnvironmentInfra> ForgeProviderService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.get_environment();
        let version = env.version();
        let retry_config = Arc::new(env.retry_config);
        Self {
            infra,
            retry_config,
            cached_client: Arc::new(RwLock::new(None)),
            version,
            timeout_config: env.http,
        }
    }
    async fn provider(&self, key: ForgeKey) -> Result<Provider> {
        self.infra
            .get_provider_infra(Some(key))
            .context("User isn't logged in")
    }

    async fn client(&self, key: ForgeKey) -> Result<Client> {
        {
            let client_guard = self.cached_client.read().await;
            if let Some(client) = client_guard.as_ref() {
                return Ok(client.clone());
            }
        }

        // Client doesn't exist, create new one
        let provider = self.provider(key).await?;
        let client = Client::new(
            provider,
            self.retry_config.clone(),
            &self.version,
            &self.timeout_config,
        )?;

        // Cache the new client
        {
            let mut client_guard = self.cached_client.write().await;
            *client_guard = Some(client.clone());
        }

        Ok(client)
    }
}

#[async_trait::async_trait]
impl<I: ProviderInfra + EnvironmentInfra> ProviderService for ForgeProviderService<I> {
    async fn chat(
        &self,
        model: &ModelId,
        request: ChatContext,
        key: ForgeKey,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let client = self.client(key).await?;

        client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self, key: ForgeKey) -> Result<Vec<Model>> {
        let client = self.client(key).await?;

        client.models().await
    }
}
