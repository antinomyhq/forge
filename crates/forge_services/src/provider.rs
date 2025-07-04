use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::ProviderService;
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, HttpConfig, Model, ModelId, Provider,
    ResultStream, RetryConfig,
};
use forge_provider::Client;
use tokio::sync::Mutex;

use crate::EnvironmentInfra;

#[derive(Clone)]
pub struct ForgeProviderService {
    retry_config: Arc<RetryConfig>,
    cached_client: Arc<Mutex<Option<Client>>>,
    version: String,
    timeout_config: HttpConfig,
}

impl ForgeProviderService {
    pub fn new<I: EnvironmentInfra>(infra: Arc<I>) -> Self {
        let env = infra.get_environment();
        let version = env.version();
        let retry_config = Arc::new(env.retry_config);
        Self {
            retry_config,
            cached_client: Arc::new(Mutex::new(None)),
            version,
            timeout_config: env.http,
        }
    }

    async fn client(&self, provider: Provider) -> Result<Client> {
        let mut client_guard = self.cached_client.lock().await;

        match client_guard.as_ref() {
            Some(client) => Ok(client.clone()),
            None => {
                // Client doesn't exist, create new one
                let client = Client::new(
                    provider,
                    self.retry_config.clone(),
                    &self.version,
                    &self.timeout_config,
                )?;

                // Cache the new client
                *client_guard = Some(client.clone());
                Ok(client)
            }
        }
    }
}

#[async_trait::async_trait]
impl ProviderService for ForgeProviderService {
    async fn chat(
        &self,
        model: &ModelId,
        request: ChatContext,
        provider: Provider,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let client = self.client(provider).await?;

        client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self, provider: Provider) -> Result<Vec<Model>> {
        let client = self.client(provider).await?;

        client.models().await
    }
}
