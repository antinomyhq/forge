use forge_domain::Provider;

use super::drop_tool_call::DropToolCalls;
use super::identity::Identity;
use super::make_openai_compat::MakeOpenAiCompat;
use super::set_cache::SetCache;
use super::tool_choice::SetToolChoice;
use super::Transformer;
use crate::forge_provider::request::Request;
use crate::forge_provider::tool_choice::ToolChoice;

/// Pipeline for transforming requests based on the provider type
pub struct ProviderPipeline<'a>(&'a Provider);

impl<'a> ProviderPipeline<'a> {
    /// Creates a new provider pipeline for the given provider
    pub fn new(provider: &'a Provider) -> Self {
        Self(provider)
    }
}

impl Transformer for ProviderPipeline<'_> {
    fn transform(&self, request: Request) -> Request {
        // Only Anthropic and Gemini requires cache configuration to be set.
        // ref: https://openrouter.ai/docs/features/prompt-caching
        let or_transformers = Identity
            .combine(DropToolCalls.when_model("mistral"))
            .combine(SetToolChoice::new(ToolChoice::Auto).when_model("gemini"))
            .combine(SetCache.when_model("gemini|anthropic"))
            .when(move |_| supports_open_router_params(self.0));

        let open_ai_compat = MakeOpenAiCompat.when(move |_| !supports_open_router_params(self.0));

        or_transformers.combine(open_ai_compat).transform(request)
    }
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider) -> bool {
    provider.is_open_router() || provider.is_antinomy()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_open_router_params() {
        assert!(supports_open_router_params(&Provider::antinomy("antinomy")));
        assert!(supports_open_router_params(&Provider::open_router(
            "open-router"
        )));

        assert!(!supports_open_router_params(&Provider::openai("openai")));
        assert!(!supports_open_router_params(&Provider::anthropic("claude")));
    }
}
