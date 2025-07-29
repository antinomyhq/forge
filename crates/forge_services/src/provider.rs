use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::ProviderService;
use forge_app::domain::{
    ChatCompletionMessage, Context as ChatContext, HttpConfig, Model, ModelId, Provider,
    ResultStream, RetryConfig,
};
use forge_provider::{Client, ClientBuilder};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

use crate::EnvironmentInfra;

// Simple mapping for conversation_id to thread_id
static THREAD_ID_MAP: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub struct ForgeProviderService {
    retry_config: Arc<RetryConfig>,
    cached_client: Arc<Mutex<Option<Client>>>,
    cached_models: Arc<Mutex<Option<Vec<Model>>>>,
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
            cached_models: Arc::new(Mutex::new(None)),
            version,
            timeout_config: env.http,
        }
    }

    async fn client(&self, provider: Provider) -> Result<Client> {
        let mut client_guard = self.cached_client.lock().await;

        match client_guard.as_ref() {
            Some(client) => Ok(client.clone()),
            None => {
                let client = ClientBuilder::new(provider, &self.version)
                    .retry_config(self.retry_config.clone())
                    .timeout_config(self.timeout_config.clone())
                    .use_hickory(false) // use native DNS resolver(GAI)
                    .build()?;

                // Cache the new client
                *client_guard = Some(client.clone());
                Ok(client)
            }
        }
    }

    /// Get or create a thread ID for Copilot conversations
    async fn get_or_create_thread_id(
        &self,
        client: &Client,
        conversation_id: &str,
    ) -> anyhow::Result<String> {
        // First check if thread_id exists in map
        let existing_thread_id = {
            let map = THREAD_ID_MAP.lock().await;
            map.get(conversation_id).cloned()
        };

        if let Some(existing_thread_id) = existing_thread_id {
            Ok(existing_thread_id)
        } else {
            // Create a new thread if none exists
            let new_thread_id = client.copilot_create_thread().await?;
            // Store the new thread_id in map
            {
                let mut map = THREAD_ID_MAP.lock().await;
                map.insert(conversation_id.to_string(), new_thread_id.clone());
            }
            Ok(new_thread_id)
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
        let client = self.client(provider.clone()).await?;

        // Handle Copilot thread ID management
        if provider.is_copilot() {
            // Get conversation_id for mapping
            let conversation_id = request.conversation_id.unwrap_or_default();
            let conversation_id_str = conversation_id.to_string();

            // Get or create thread ID
            let thread_id = self
                .get_or_create_thread_id(&client, &conversation_id_str)
                .await?;

            // Pass the thread ID to the chat function
            return client.chat(model, request, Some(thread_id)).await;
        }

        client
            .chat(model, request, None)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self, provider: Provider) -> Result<Vec<Model>> {
        // Check cache first
        {
            let models_guard = self.cached_models.lock().await;
            if let Some(cached_models) = models_guard.as_ref() {
                return Ok(cached_models.clone());
            }
        }

        // Models not in cache, fetch from client
        let client = self.client(provider).await?;
        let models = client.models().await?;

        // Cache the models
        {
            let mut models_guard = self.cached_models.lock().await;
            *models_guard = Some(models.clone());
        }

        Ok(models)
    }
}
