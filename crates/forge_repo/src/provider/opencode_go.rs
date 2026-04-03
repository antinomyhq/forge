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

use crate::provider::openai::OpenAIResponseRepository;

#[derive(Setters)]
#[setters(strip_option, into)]
pub struct OpenCodeGoResponseRepository<F> {
    openai_repo: OpenAIResponseRepository<F>,
    retry_config: Arc<RetryConfig>,
}

impl<F: HttpInfra + Sync> OpenCodeGoResponseRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            openai_repo: OpenAIResponseRepository::new(infra),
            retry_config: Arc::new(RetryConfig::default()),
        }
    }

    fn build_provider(&self, provider: &Provider<Url>) -> Provider<Url> {
        let mut new_provider = provider.clone();

        new_provider.url = Url::parse("https://opencode.ai/zen/go/v1/chat/completions").unwrap();
        new_provider.response = Some(ProviderResponse::OpenAI);

        new_provider
    }
}

impl<F: HttpInfra + Sync> OpenCodeGoResponseRepository<F> {
    pub async fn chat(
        &self,
        model_id: &ModelId,
        context: ChatContext,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let adapted_provider = self.build_provider(&provider);

        self.openai_repo
            .chat(model_id, context, adapted_provider)
            .await
    }

    pub async fn models(&self, provider: Provider<Url>) -> Result<Vec<Model>> {
        if let Some(models) = provider.models() {
            match models {
                forge_domain::ModelSource::Hardcoded(models) => Ok(models.clone()),
                forge_domain::ModelSource::Url(_) => {
                    Ok(vec![])
                }
            }
        } else {
            Ok(vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use std::str::FromStr;
    use url::Url;

    use forge_app::domain::ProviderResponse;
    use forge_domain::ProviderId;

    #[test]
    fn test_opencode_go_provider_url() {
        let url = Url::parse("https://opencode.ai/zen/go/v1/chat/completions").unwrap();
        assert_eq!(url.as_str(), "https://opencode.ai/zen/go/v1/chat/completions");
    }

    #[test]
    fn test_opencode_go_provider_id_from_str() {
        let actual = ProviderId::from_str("opencode_go").unwrap();
        let expected = ProviderId::OPENCODE_GO;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_opencode_go_response_type() {
        let response = ProviderResponse::OpenAI;
        assert_eq!(format!("{:?}", response), "OpenAI");
    }
}
