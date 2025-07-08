use std::{sync::Arc, vec};

use anyhow::{Context, Result};
use forge_app::{AppConfig, ProviderService};
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, HttpConfig, Model, ModelId, Provider, ProviderConfig, ProviderDetails, ResultStream, RetryConfig, Workflow
};
use forge_provider::Client;
use tokio::sync::RwLock;

use crate::EnvironmentInfra;

#[derive(Clone)]
pub struct ForgeProviderService {
    retry_config: Arc<RetryConfig>,
    cached_client: Arc<RwLock<Option<Client>>>,
    version: String,
    timeout_config: HttpConfig,
    providers: Arc<RwLock<Vec<ProviderDetails>>>,
}

impl ForgeProviderService {
    pub fn new<I: EnvironmentInfra>(infra: Arc<I>, app_config: AppConfig) -> Self {
        let env = infra.get_environment();
        let version = env.version();
        let retry_config = Arc::new(env.retry_config);
        
        // Start with default providers
        let mut providers: Vec<ProviderDetails> = ProviderConfig::default();
        // Try to load and merge providers from forge.yaml
        if let Some(workflow) = Self::load_forge_workflow(&infra) {
            if !workflow.provider_config.is_empty() {
                for forge_provider in workflow.provider_config {
                    if let Some(existing_idx) = providers.iter().position(|p| p.id == forge_provider.id) {
                        providers[existing_idx] = forge_provider;
                    } else {
                        providers.push(forge_provider);
                    }
                }
            }
        }
        let mut resolved_providers = resolve_env_provider(&providers, infra.as_ref());
        // Check if a provider with forge is already there
            let has_forge_provider = resolved_providers.iter()
                .any(|p| p.id.to_lowercase().contains("forge"));

            if !has_forge_provider {
                if let Some(login) = app_config.key_info {
                    let forge_provider = ProviderDetails::new(
                        "forge".to_string(),
                        "Forge".to_string(),
                        "Forge AI Provider".to_string(),
                        login.api_key,
                        "openai".to_string(),
                        " https://antinomy.ai/api/v1/".to_string(),
                    );
                    resolved_providers.push(forge_provider);
                }
            }
        Self {
            retry_config,
            cached_client: Arc::new(RwLock::new(None)),
            version,
            timeout_config: env.http,
            providers: Arc::new(RwLock::new(resolved_providers)),
        }
    }

    fn load_forge_workflow<I: EnvironmentInfra>(infra: &Arc<I>) -> Option<Workflow> {
        let env = infra.get_environment();
        let forge_path = env.cwd.join("forge.yaml");
        
        if forge_path.exists() {
            match std::fs::read_to_string(&forge_path) {
                Ok(content) => {
                    match serde_yml::from_str::<Workflow>(&content) {
                        Ok(workflow) => Some(workflow),
                        Err(_) => None, // Failed to parse, return None
                    }
                }
                Err(_) => None, // Failed to read file, return None
            }
        } else {
            None // File doesn't exist, return None
        }
    }

    async fn client(&self, provider: Provider) -> Result<Client> {
        {
            let client_guard = self.cached_client.read().await;
            if let Some(client) = client_guard.as_ref() {
                return Ok(client.clone());
            }
        }

        // Client doesn't exist, create new one
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

    async fn new_client(&self, provider: Provider) -> Result<Client> {
        // Client doesn't exist, create new one
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
        let client = self.new_client(provider).await?;

        client.models().await
    }

    fn providers(&self) -> Vec<ProviderDetails> {
        // This is problematic - we can't await in a sync method
        // We need to make this method async or change the trait
        // For now, let's block on the future
        let providers = self.providers.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                providers.read().await.clone()
            })
        })
    }

    fn update_available_providers(&self, provider: ProviderDetails) {
        let providers = self.providers.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut providers_guard = providers.write().await;
                if let Some(existing) = providers_guard.iter_mut().find(|p| p.id == provider.id) {
                    *existing = provider;
                } else {
                    providers_guard.push(provider);
                }
            })
        })
    }

    async fn update_provider(&self, provider: Provider) -> Result<()> {
        println!("Updating provider: {}", provider.id());
        
        // Check if we have a cached client
        let has_cached_client = {
            let client_guard = self.cached_client.read().await;
            client_guard.is_some()
        };
        
        if has_cached_client {
            // Update the existing cached client
            let mut client_guard = self.cached_client.write().await;
            if let Some(client) = client_guard.as_mut() {
                println!("Using cached client for provider: {}", provider.id());
                client.update_provider(provider).await;
            }
        } else {
            // Create a new client (this will cache it automatically)
            println!("No cached client found, creating a new one");
            let _new_client = self.new_client(provider).await?;
        }
        Ok(())
    }
}

pub fn resolve_env_provider<F: EnvironmentInfra>(
    provider_config: &Vec<ProviderDetails>,
    env: &F
) -> Vec<ProviderDetails> {
    let mut updated_config = vec![];
    for provider in provider_config {
        let api_key = env.get_env_var(&provider.api_key);
        if let Some(api_key) = api_key {
            let mut updated_provider = provider.clone();
            updated_provider.api_key = api_key;
            updated_config.push(updated_provider);
        }
    }

    updated_config
}