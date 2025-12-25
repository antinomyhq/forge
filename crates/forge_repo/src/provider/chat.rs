use std::sync::Arc;

use forge_app::domain::{
    ChatCompletionMessage, Context, Model, ModelId, ProviderResponse, ResultStream,
};
use forge_app::{EnvironmentInfra, HttpInfra};
use forge_domain::{ChatRepository, Provider};
use url::Url;

use crate::provider::anthropic::AnthropicResponseRepository;
use crate::provider::bedrock::BedrockResponseRepository;
use crate::provider::openai::OpenAIResponseRepository;

/// Repository responsible for routing chat requests to the appropriate provider
/// implementation based on the provider's response type.
pub struct ForgeChatRepository<F> {
    openai_repo: OpenAIResponseRepository<F>,
    anthropic_repo: AnthropicResponseRepository<F>,
    bedrock_repo: BedrockResponseRepository,
}

impl<F: EnvironmentInfra + HttpInfra> ForgeChatRepository<F> {
    /// Creates a new ForgeChatRepository with the given infrastructure.
    ///
    /// # Arguments
    ///
    /// * `infra` - Infrastructure providing environment and HTTP capabilities
    pub fn new(infra: Arc<F>) -> Self {
        let env = infra.get_environment();
        let retry_config = Arc::new(env.retry_config.clone());

        let openai_repo =
            OpenAIResponseRepository::new(infra.clone()).retry_config(retry_config.clone());

        let anthropic_repo =
            AnthropicResponseRepository::new(infra.clone()).retry_config(retry_config.clone());

        let bedrock_repo = BedrockResponseRepository::new(retry_config);

        Self { openai_repo, anthropic_repo, bedrock_repo }
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + HttpInfra + Sync> ChatRepository for ForgeChatRepository<F> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        // Route based on provider response type
        match provider.response {
            Some(ProviderResponse::OpenAI) => {
                self.openai_repo.chat(model_id, context, provider).await
            }
            Some(ProviderResponse::Anthropic) => {
                self.anthropic_repo.chat(model_id, context, provider).await
            }
            Some(ProviderResponse::Bedrock) => {
                self.bedrock_repo.chat(model_id, context, provider).await
            }
            None => Err(anyhow::anyhow!(
                "Provider response type not configured for provider: {}",
                provider.id
            )),
        }
    }

    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        // Route based on provider response type
        match provider.response {
            Some(ProviderResponse::OpenAI) => self.openai_repo.models(provider).await,
            Some(ProviderResponse::Anthropic) => self.anthropic_repo.models(provider).await,
            Some(ProviderResponse::Bedrock) => self.bedrock_repo.models(provider).await,
            None => Err(anyhow::anyhow!(
                "Provider response type not configured for provider: {}",
                provider.id
            )),
        }
    }
}
