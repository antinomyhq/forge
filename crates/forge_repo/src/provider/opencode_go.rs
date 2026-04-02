use std::sync::Arc;

use anyhow::Result;
use forge_app::HttpInfra;
use forge_app::domain::{
    ChatCompletionMessage, Context as ChatContext, Model, ModelId, Provider, ProviderResponse,
    ResultStream,
};
use forge_config::RetryConfig;
use forge_domain::ChatRepository;
use url::Url;

use crate::provider::anthropic::AnthropicResponseRepository;
use crate::provider::openai::OpenAIResponseRepository;

/// Repository for the OpenCode Go gateway.
///
/// OpenCode Go exposes a small curated model list, but the models are not all
/// served through the same upstream protocol. GLM and Kimi use an
/// OpenAI-compatible chat completions endpoint while MiniMax models use an
/// Anthropic-compatible messages endpoint.
pub struct OpenCodeGoResponseRepository<F> {
    openai_repo: OpenAIResponseRepository<F>,
    anthropic_repo: AnthropicResponseRepository<F>,
}

impl<F: HttpInfra + Sync> OpenCodeGoResponseRepository<F> {
    /// Creates a new OpenCode Go repository backed by the shared HTTP infra.
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            openai_repo: OpenAIResponseRepository::new(infra.clone()),
            anthropic_repo: AnthropicResponseRepository::new(infra.clone()),
        }
    }

    /// Pushes the shared retry policy into each delegated upstream repository.
    pub fn retry_config(mut self, retry_config: Arc<RetryConfig>) -> Self {
        // Keep wrapper and delegate behavior aligned so route selection does
        // not silently bypass configured retry policy.
        self.openai_repo = self.openai_repo.retry_config(retry_config.clone());
        self.anthropic_repo = self.anthropic_repo.retry_config(retry_config);
        self
    }

    /// Selects the upstream protocol family required by the target model.
    fn get_backend(&self, model_id: &ModelId) -> OpenCodeGoBackend {
        match model_id.as_str() {
            "minimax-m2.5" | "minimax-m2.7" => OpenCodeGoBackend::Anthropic,
            _ => OpenCodeGoBackend::OpenAI,
        }
    }

    /// Rewrites the configured provider to the endpoint shape expected by the
    /// selected upstream protocol.
    fn build_provider(&self, provider: &Provider<Url>, model_id: &ModelId) -> Provider<Url> {
        let backend = self.get_backend(model_id);
        let mut new_provider = provider.clone();

        // Pin the provider to the concrete upstream contract before delegating
        // to the protocol-specific repository.
        match backend {
            OpenCodeGoBackend::Anthropic => {
                new_provider.url = Url::parse("https://opencode.ai/zen/go/v1/messages").unwrap();
                new_provider.response = Some(ProviderResponse::Anthropic);
            }
            OpenCodeGoBackend::OpenAI => {
                new_provider.url =
                    Url::parse("https://opencode.ai/zen/go/v1/chat/completions").unwrap();
                new_provider.response = Some(ProviderResponse::OpenAI);
            }
        }

        new_provider
    }

    /// Routes the chat request through the translator that matches the selected
    /// OpenCode Go endpoint family.
    pub async fn chat(
        &self,
        model_id: &ModelId,
        context: ChatContext,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let backend = self.get_backend(model_id);
        let adapted_provider = self.build_provider(&provider, model_id);

        match backend {
            OpenCodeGoBackend::Anthropic => {
                self.anthropic_repo
                    .chat(model_id, context, adapted_provider)
                    .await
            }
            OpenCodeGoBackend::OpenAI => {
                self.openai_repo
                    .chat(model_id, context, adapted_provider)
                    .await
            }
        }
    }

    /// Returns the curated model list embedded in `provider.json`.
    pub async fn models(&self, provider: Provider<Url>) -> Result<Vec<Model>> {
        // Keep model discovery static because OpenCode Go publishes a curated
        // list rather than a general-purpose models endpoint contract.
        if let Some(models) = provider.models() {
            match models {
                forge_domain::ModelSource::Hardcoded(models) => Ok(models.clone()),
                forge_domain::ModelSource::Url(_) => Ok(vec![]),
            }
        } else {
            Ok(vec![])
        }
    }
}

/// The protocol families exposed behind the OpenCode Go gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenCodeGoBackend {
    OpenAI,
    Anthropic,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Mirrors the production routing table so tests can assert behavior
    /// without constructing the full repository.
    fn get_backend_for_test(model_id: &str) -> OpenCodeGoBackend {
        match model_id {
            "minimax-m2.5" | "minimax-m2.7" => OpenCodeGoBackend::Anthropic,
            _ => OpenCodeGoBackend::OpenAI,
        }
    }

    /// Builds a minimal provider fixture for endpoint rewrite tests.
    fn provider_fixture() -> Provider<Url> {
        Provider {
            id: forge_domain::ProviderId::OPENCODE_GO,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenCodeGo),
            url: Url::parse("https://opencode.ai/zen/go/v1/chat/completions").unwrap(),
            models: None,
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: None,
            custom_headers: None,
        }
    }

    #[test]
    fn test_model_routing() {
        let actual = get_backend_for_test("glm-5");
        let expected = OpenCodeGoBackend::OpenAI;
        assert_eq!(actual, expected);

        let actual = get_backend_for_test("kimi-k2.5");
        let expected = OpenCodeGoBackend::OpenAI;
        assert_eq!(actual, expected);

        let actual = get_backend_for_test("minimax-m2.5");
        let expected = OpenCodeGoBackend::Anthropic;
        assert_eq!(actual, expected);

        let actual = get_backend_for_test("minimax-m2.7");
        let expected = OpenCodeGoBackend::Anthropic;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_build_provider_rewrites_openai_models() {
        let fixture = provider_fixture();
        let repository =
            OpenCodeGoResponseRepository::new(Arc::new(forge_repo_test_support::NoopHttpInfra));

        let actual = repository.build_provider(&fixture, &ModelId::from("glm-5"));
        let expected_url = Url::parse("https://opencode.ai/zen/go/v1/chat/completions").unwrap();
        let expected_response = Some(ProviderResponse::OpenAI);

        assert_eq!(actual.url, expected_url);
        assert_eq!(actual.response, expected_response);
    }

    #[test]
    fn test_build_provider_rewrites_anthropic_models() {
        let fixture = provider_fixture();
        let repository =
            OpenCodeGoResponseRepository::new(Arc::new(forge_repo_test_support::NoopHttpInfra));

        let actual = repository.build_provider(&fixture, &ModelId::from("minimax-m2.7"));
        let expected_url = Url::parse("https://opencode.ai/zen/go/v1/messages").unwrap();
        let expected_response = Some(ProviderResponse::Anthropic);

        assert_eq!(actual.url, expected_url);
        assert_eq!(actual.response, expected_response);
    }

    #[test]
    fn test_retry_config_returns_repository() {
        let fixture = Arc::new(RetryConfig::default());
        let actual =
            OpenCodeGoResponseRepository::new(Arc::new(forge_repo_test_support::NoopHttpInfra))
                .retry_config(fixture);

        let expected = OpenCodeGoBackend::OpenAI;
        let actual_backend = actual.get_backend(&ModelId::from("glm-5"));
        assert_eq!(actual_backend, expected);
    }

    mod forge_repo_test_support {
        use bytes::Bytes;
        use forge_app::HttpInfra;
        use reqwest::header::HeaderMap;
        use reqwest_eventsource::EventSource;
        use url::Url;

        /// Minimal HTTP infra marker used to satisfy the repository type in
        /// pure unit tests.
        #[derive(Clone)]
        pub struct NoopHttpInfra;

        #[async_trait::async_trait]
        impl HttpInfra for NoopHttpInfra {
            async fn http_get(
                &self,
                _url: &Url,
                _headers: Option<HeaderMap>,
            ) -> anyhow::Result<reqwest::Response> {
                unreachable!("routing tests should not perform HTTP GET")
            }

            async fn http_post(
                &self,
                _url: &Url,
                _headers: Option<HeaderMap>,
                _body: Bytes,
            ) -> anyhow::Result<reqwest::Response> {
                unreachable!("routing tests should not perform HTTP POST")
            }

            async fn http_delete(&self, _url: &Url) -> anyhow::Result<reqwest::Response> {
                unreachable!("routing tests should not perform HTTP DELETE")
            }

            async fn http_eventsource(
                &self,
                _url: &Url,
                _headers: Option<HeaderMap>,
                _body: Bytes,
            ) -> anyhow::Result<EventSource> {
                unreachable!("routing tests should not open event streams")
            }
        }
    }
}
