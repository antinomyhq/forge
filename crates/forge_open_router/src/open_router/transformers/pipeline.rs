use forge_domain::Provider;

use super::drop_or_fields::DropOpenRouterFields;
use super::drop_tool_call::DropToolCalls;
use super::identity::Identity;
use super::set_cache::SetCache;
use super::tool_choice::SetToolChoice;
use super::Transformer;
use crate::open_router::request::OpenRouterRequest;
use crate::open_router::tool_choice::ToolChoice;

/// Pipeline for transforming requests based on the provider type
pub struct ProviderPipeline<'a>(&'a Provider);

impl<'a> ProviderPipeline<'a> {
    /// Creates a new provider pipeline for the given provider
    pub fn new(provider: &'a Provider) -> Self {
        Self(provider)
    }
}

impl Transformer for ProviderPipeline<'_> {
    fn transform(&self, request: OpenRouterRequest) -> OpenRouterRequest {
        let or_transformers = Identity
            .combine(DropToolCalls.when_model("mistral"))
            .combine(SetToolChoice::new(ToolChoice::Auto).when_model("gemini"))
            .combine(SetCache.except_when_model("mistral|gemini|openai"))
            .when(move |_| self.0.is_open_router());

        let non_open_router = DropOpenRouterFields.when(move |_| !self.0.is_open_router());

        or_transformers.combine(non_open_router).transform(request)
    }
}
