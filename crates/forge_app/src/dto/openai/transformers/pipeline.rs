use forge_domain::{DefaultTransformation, Provider, Transformer};

use super::drop_tool_call::DropToolCalls;
use super::make_openai_compat::MakeOpenAiCompat;
use super::set_cache::SetCache;
use super::tool_choice::SetToolChoice;
use super::when_model::when_model;
use crate::dto::openai::{Request, ToolChoice};

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
        let or_transformers = DefaultTransformation::<Request>::new()
            .pipe(DropToolCalls.when(when_model("mistral")))
            .pipe(SetToolChoice::new(ToolChoice::Auto).when(when_model("gemini")))
            .pipe(SetCache.when(when_model("gemini|anthropic")))
            .when(move |_| supports_open_router_params(provider));

        let open_ai_compat = MakeOpenAiCompat.when(move |_| !supports_open_router_params(provider));

        let mut combined = or_transformers.pipe(open_ai_compat);
        combined.transform(request)
    }
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider) -> bool {
    provider.is_open_router() || provider.is_forge()
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
