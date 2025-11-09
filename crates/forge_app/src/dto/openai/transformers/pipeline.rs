use forge_domain::{DefaultTransformation, Provider, ProviderId, Transformer};
use url::Url;

use super::drop_tool_call::DropToolCalls;
use super::make_cerebras_compat::MakeCerebrasCompat;
use super::make_openai_compat::MakeOpenAiCompat;
use super::normalize_tool_schema::NormalizeToolSchema;
use super::set_cache::SetCache;
use super::tool_choice::SetToolChoice;
use super::when_model::when_model;
use super::zai_reasoning::SetZaiThinking;
use crate::dto::openai::{Request, ToolChoice};

/// Pipeline for transforming requests based on the provider type
pub struct ProviderPipeline<'a>(&'a Provider<Url>);

impl<'a> ProviderPipeline<'a> {
    /// Creates a new provider pipeline for the given provider
    pub fn new(provider: &'a Provider<Url>) -> Self {
        Self(provider)
    }
}

impl Transformer for ProviderPipeline<'_> {
    type Value = Request;

    fn transform(&mut self, request: Self::Value) -> Self::Value {
        // Only Anthropic and Gemini requires cache configuration to be set.
        // ref: https://openrouter.ai/docs/features/prompt-caching
        let provider = self.0;

        // Z.ai transformer must run before MakeOpenAiCompat which removes reasoning
        // field
        let zai_thinking = SetZaiThinking.when(move |_| is_zai_provider(provider));

        let or_transformers = DefaultTransformation::<Request>::new()
            .pipe(DropToolCalls.when(when_model("mistral")))
            .pipe(SetToolChoice::new(ToolChoice::Auto).when(when_model("gemini")))
            .pipe(SetCache.when(when_model("gemini|anthropic")))
            .when(move |_| supports_open_router_params(provider));

        let open_ai_compat = MakeOpenAiCompat.when(move |_| !supports_open_router_params(provider));

        let cerebras_compat =
            MakeCerebrasCompat.when(move |_| provider.id == ProviderId::CEREBRAS_STR);

        let mut combined = zai_thinking
            .pipe(or_transformers)
            .pipe(open_ai_compat)
            .pipe(cerebras_compat)
            .pipe(NormalizeToolSchema);
        combined.transform(request)
    }
}

/// Checks if provider is a z.ai provider (zai or zai_coding)
fn is_zai_provider(provider: &Provider<Url>) -> bool {
    provider.id == ProviderId::ZAI_STR || provider.id == ProviderId::ZAI_CODING_STR
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider<Url>) -> bool {
    provider.id == ProviderId::OPEN_ROUTER_STR
        || provider.id == ProviderId::FORGE_STR
        || provider.id == ProviderId::ZAI_STR
        || provider.id == ProviderId::ZAI_CODING_STR
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use url::Url;

    use super::*;
    use crate::domain::{Models, ProviderResponse};

    // Test helper functions
    fn make_credential(provider_id: ProviderId, key: &str) -> Option<forge_domain::AuthCredential> {
        Some(forge_domain::AuthCredential {
            id: provider_id,
            auth_details: forge_domain::AuthDetails::ApiKey(forge_domain::ApiKey::from(
                key.to_string(),
            )),
            url_params: HashMap::new(),
        })
    }

    fn forge(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::forge(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://antinomy.ai/api/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::forge(), key),
            models: Models::Url(Url::parse("https://antinomy.ai/api/v1/models").unwrap()),
        }
    }

    fn zai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::zai(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::zai(), key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    fn zai_coding(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::zai_coding(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::zai_coding(), key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::openai(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::openai(), key),
            models: Models::Url(Url::parse("https://api.openai.com/v1/models").unwrap()),
        }
    }

    fn xai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::xai(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::xai(), key),
            models: Models::Url(Url::parse("https://api.x.ai/v1/models").unwrap()),
        }
    }

    fn requesty(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::requesty(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.requesty.ai/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::requesty(), key),
            models: Models::Url(Url::parse("https://api.requesty.ai/v1/models").unwrap()),
        }
    }

    fn open_router(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::open_router(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://openrouter.ai/api/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::open_router(), key),
            models: Models::Url(Url::parse("https://openrouter.ai/api/v1/models").unwrap()),
        }
    }

    fn anthropic(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::anthropic(),
            response: ProviderResponse::Anthropic,
            url: Url::parse("https://api.anthropic.com/v1/messages").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::anthropic(), key),
            models: Models::Url(Url::parse("https://api.anthropic.com/v1/models").unwrap()),
        }
    }

    #[test]
    fn test_supports_open_router_params() {
        assert!(supports_open_router_params(&forge("forge")));
        assert!(supports_open_router_params(&open_router("open-router")));

        assert!(!supports_open_router_params(&openai("openai")));
        assert!(!supports_open_router_params(&requesty("requesty")));
        assert!(!supports_open_router_params(&xai("xai")));
        assert!(!supports_open_router_params(&anthropic("claude")));
    }

    #[test]
    fn test_is_zai_provider() {
        assert!(is_zai_provider(&zai("zai")));
        assert!(is_zai_provider(&zai_coding("zai-coding")));

        assert!(!is_zai_provider(&openai("openai")));
        assert!(!is_zai_provider(&anthropic("claude")));
        assert!(!is_zai_provider(&open_router("open-router")));
    }

    #[test]
    fn test_zai_provider_applies_thinking_transformation() {
        let provider = zai("zai");
        let fixture = Request::default().reasoning(forge_domain::ReasoningConfig {
            enabled: Some(true),
            effort: None,
            max_tokens: None,
            exclude: None,
        });

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        assert!(actual.thinking.is_some());
        assert_eq!(
            actual.thinking.unwrap().r#type,
            crate::dto::openai::ThinkingType::Enabled
        );
        assert_eq!(actual.reasoning, None);
    }

    #[test]
    fn test_zai_coding_provider_applies_thinking_transformation() {
        let provider = zai_coding("zai-coding");
        let fixture = Request::default().reasoning(forge_domain::ReasoningConfig {
            enabled: Some(true),
            effort: None,
            max_tokens: None,
            exclude: None,
        });

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        assert!(actual.thinking.is_some());
        assert_eq!(
            actual.thinking.unwrap().r#type,
            crate::dto::openai::ThinkingType::Enabled
        );
        assert_eq!(actual.reasoning, None);
    }

    #[test]
    fn test_non_zai_provider_doesnt_apply_thinking_transformation() {
        let provider = openai("openai");
        let fixture = Request::default().reasoning(forge_domain::ReasoningConfig {
            enabled: Some(true),
            effort: None,
            max_tokens: None,
            exclude: None,
        });

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        assert_eq!(actual.thinking, None);
        // OpenAI compat transformer removes reasoning field
        assert_eq!(actual.reasoning, None);
    }
}
