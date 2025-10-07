use forge_domain::{DefaultTransformation, Transformer};

use super::drop_tool_call::DropToolCalls;
use super::make_cerebras_compat::MakeCerebrasCompat;
use super::make_openai_compat::MakeOpenAiCompat;
use super::set_cache::SetCache;
use super::tool_choice::SetToolChoice;
use super::when_model::when_model;
use super::zai_reasoning::SetZaiThinking;
use crate::dto::openai::{Request, ToolChoice};
use crate::dto::{Provider, ProviderId};

/// Pipeline for transforming requests based on the provider type
pub struct ProviderPipeline<'a>(&'a Provider);

impl<'a> ProviderPipeline<'a> {
    /// Creates a new provider pipeline for the given provider
    pub fn new(provider: &'a Provider) -> Self {
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

        let cerebras_compat = MakeCerebrasCompat.when(move |_| provider.id == ProviderId::Cerebras);

        let mut combined = zai_thinking
            .pipe(or_transformers)
            .pipe(open_ai_compat)
            .pipe(cerebras_compat);
        combined.transform(request)
    }
}

/// Checks if provider is a z.ai provider (zai or zai_coding)
fn is_zai_provider(provider: &Provider) -> bool {
    provider.id == ProviderId::Zai || provider.id == ProviderId::ZaiCoding
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider) -> bool {
    provider.id == ProviderId::OpenRouter
        || provider.id == ProviderId::Forge
        || provider.id == ProviderId::Zai
        || provider.id == ProviderId::ZaiCoding
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_open_router_params() {
        assert!(supports_open_router_params(&Provider::forge("forge")));
        assert!(supports_open_router_params(&Provider::open_router(
            "open-router"
        )));

        assert!(!supports_open_router_params(&Provider::openai("openai")));
        assert!(!supports_open_router_params(&Provider::requesty(
            "requesty"
        )));
        assert!(!supports_open_router_params(&Provider::xai("xai")));
        assert!(!supports_open_router_params(&Provider::anthropic("claude")));
    }
}

#[test]
fn test_is_zai_provider() {
    assert!(is_zai_provider(&Provider::zai("zai")));
    assert!(is_zai_provider(&Provider::zai_coding("zai-coding")));

    assert!(!is_zai_provider(&Provider::openai("openai")));
    assert!(!is_zai_provider(&Provider::anthropic("claude")));
    assert!(!is_zai_provider(&Provider::open_router("open-router")));
}

#[test]
fn test_zai_provider_applies_thinking_transformation() {
    let provider = Provider::zai("zai");
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
    let provider = Provider::zai_coding("zai-coding");
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
    let provider = Provider::openai("openai");
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
