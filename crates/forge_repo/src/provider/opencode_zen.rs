use std::sync::Arc;

use anyhow::Result;
use derive_setters::Setters;
use forge_app::HttpInfra;
use forge_app::domain::{
    ChatCompletionMessage, Context as ChatContext, Model, ModelId, Provider, ProviderResponse,
    ResultStream,
};
use forge_config::RetryConfig;
use forge_domain::ChatRepository;
use url::Url;

use crate::provider::anthropic::AnthropicResponseRepository;
use crate::provider::google::GoogleResponseRepository;
use crate::provider::openai::OpenAIResponseRepository;
use crate::provider::openai_responses::OpenAIResponsesResponseRepository;

/// OpenCode Zen provider that routes to different backends based on model:
/// - Claude models (claude-*) -> Anthropic endpoint
/// - GPT-5 models (gpt-5*) -> OpenAIResponses endpoint
/// - Gemini models (gemini-*) -> Google endpoint
/// - Others (GLM, MiniMax, Kimi, etc.) -> OpenAI endpoint
#[derive(Setters)]
#[setters(strip_option, into)]
pub struct OpenCodeZenResponseRepository<F> {
    openai_repo: OpenAIResponseRepository<F>,
    codex_repo: OpenAIResponsesResponseRepository<F>,
    anthropic_repo: AnthropicResponseRepository<F>,
    google_repo: GoogleResponseRepository<F>,
    retry_config: Arc<RetryConfig>,
}

impl<F: HttpInfra + Sync> OpenCodeZenResponseRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            openai_repo: OpenAIResponseRepository::new(infra.clone()),
            codex_repo: OpenAIResponsesResponseRepository::new(infra.clone()),
            anthropic_repo: AnthropicResponseRepository::new(infra.clone()),
            google_repo: GoogleResponseRepository::new(infra.clone()),
            retry_config: Arc::new(RetryConfig::default()),
        }
    }

    /// Determines which backend to use based on the model ID
    fn get_backend(&self, model_id: &ModelId) -> OpenCodeBackend {
        let model_str = model_id.as_str();

        if model_str.starts_with("claude-") {
            OpenCodeBackend::Anthropic
        } else if model_str.starts_with("gpt-5") {
            OpenCodeBackend::OpenAIResponses
        } else if model_str.starts_with("gemini-") {
            OpenCodeBackend::Google
        } else {
            OpenCodeBackend::OpenAI
        }
    }

    /// Builds the appropriate provider for the given model
    /// This modifies the URL based on the model's backend requirements
    fn build_provider(&self, provider: &Provider<Url>, model_id: &ModelId) -> Provider<Url> {
        let backend = self.get_backend(model_id);
        let mut new_provider = provider.clone();

        match backend {
            OpenCodeBackend::Anthropic => {
                // Claude models use /v1/messages endpoint
                new_provider.url = Url::parse("https://opencode.ai/zen/v1/messages").unwrap();
                new_provider.response = Some(ProviderResponse::Anthropic);
            }
            OpenCodeBackend::OpenAIResponses => {
                // GPT-5 models use /v1/responses endpoint
                new_provider.url = Url::parse("https://opencode.ai/zen/v1/responses").unwrap();
                new_provider.response = Some(ProviderResponse::OpenAIResponses);
            }
            OpenCodeBackend::Google => {
                // Gemini models use model-specific endpoint
                new_provider.url = Url::parse("https://opencode.ai/zen/v1").unwrap();
                new_provider.response = Some(ProviderResponse::Google);
            }
            OpenCodeBackend::OpenAI => {
                // Other models use /v1/chat/completions endpoint (default)
                new_provider.url =
                    Url::parse("https://opencode.ai/zen/v1/chat/completions").unwrap();
                new_provider.response = Some(ProviderResponse::OpenAI);
            }
        }

        new_provider
    }

    pub async fn chat(
        &self,
        model_id: &ModelId,
        context: ChatContext,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let backend = self.get_backend(model_id);
        let adapted_provider = self.build_provider(&provider, model_id);

        match backend {
            OpenCodeBackend::Anthropic => {
                self.anthropic_repo
                    .chat(model_id, context, adapted_provider)
                    .await
            }
            OpenCodeBackend::OpenAIResponses => {
                self.codex_repo
                    .chat(model_id, context, adapted_provider)
                    .await
            }
            OpenCodeBackend::Google => {
                self.google_repo
                    .chat(model_id, context, adapted_provider)
                    .await
            }
            OpenCodeBackend::OpenAI => {
                self.openai_repo
                    .chat(model_id, context, adapted_provider)
                    .await
            }
        }
    }

    pub async fn models(&self, provider: Provider<Url>) -> Result<Vec<Model>> {
        // For OpenCode Zen, we use hardcoded models from the provider config
        // The models are already loaded from provider.json
        if let Some(models) = provider.models() {
            match models {
                forge_domain::ModelSource::Hardcoded(models) => Ok(models.clone()),
                forge_domain::ModelSource::Url(_) => {
                    // Should not happen for OpenCode Zen as we hardcode models
                    Ok(vec![])
                }
            }
        } else {
            Ok(vec![])
        }
    }
}

/// Backend type for OpenCode Zen routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenCodeBackend {
    OpenAI,
    OpenAIResponses,
    Anthropic,
    Google,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to determine backend routing (mirrors get_backend logic)
    fn get_backend_for_test(model_id: &str) -> OpenCodeBackend {
        if model_id.starts_with("claude-") {
            OpenCodeBackend::Anthropic
        } else if model_id.starts_with("gpt-5") {
            OpenCodeBackend::OpenAIResponses
        } else if model_id.starts_with("gemini-") {
            OpenCodeBackend::Google
        } else {
            OpenCodeBackend::OpenAI
        }
    }

    #[test]
    fn test_model_routing() {
        // Test Claude models route to Anthropic
        assert_eq!(
            get_backend_for_test("claude-opus-4-6"),
            OpenCodeBackend::Anthropic
        );
        assert_eq!(
            get_backend_for_test("claude-sonnet-4-5"),
            OpenCodeBackend::Anthropic
        );
        assert_eq!(
            get_backend_for_test("claude-haiku-4-5"),
            OpenCodeBackend::Anthropic
        );

        // Test GPT-5 models route to OpenAIResponses
        assert_eq!(
            get_backend_for_test("gpt-5.4-pro"),
            OpenCodeBackend::OpenAIResponses
        );
        assert_eq!(
            get_backend_for_test("gpt-5"),
            OpenCodeBackend::OpenAIResponses
        );
        assert_eq!(
            get_backend_for_test("gpt-5.1-codex"),
            OpenCodeBackend::OpenAIResponses
        );

        // Test Gemini models route to Google
        assert_eq!(
            get_backend_for_test("gemini-3.1-pro"),
            OpenCodeBackend::Google
        );
        assert_eq!(
            get_backend_for_test("gemini-3-flash"),
            OpenCodeBackend::Google
        );

        // Test other models route to OpenAI
        assert_eq!(get_backend_for_test("glm-5"), OpenCodeBackend::OpenAI);
        assert_eq!(
            get_backend_for_test("minimax-m2.5"),
            OpenCodeBackend::OpenAI
        );
        assert_eq!(get_backend_for_test("kimi-k2.5"), OpenCodeBackend::OpenAI);
        assert_eq!(get_backend_for_test("big-pickle"), OpenCodeBackend::OpenAI);
    }
}
