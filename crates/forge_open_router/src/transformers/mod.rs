mod drop_tool_call;
mod set_cache;
mod tool_choice;

use drop_tool_call::DropToolCalls;
use set_cache::SetCache;
use tool_choice::SetToolChoice;

use crate::request::OpenRouterRequest;
use crate::tool_choice::ToolChoice;

/// A trait for transforming OpenRouterRequest based on model-specific
/// requirements
pub trait Transformer {
    /// Transform the request according to the transformer's logic
    fn transform(&self, request: OpenRouterRequest) -> OpenRouterRequest;
    fn combine<Other>(self, other: Other) -> Combine<Self, Other>
    where
        Self: Sized,
    {
        Combine(self, other)
    }

    fn when<F: Fn(&OpenRouterRequest) -> bool>(self, condition: F) -> When<Self, F>
    where
        Self: Sized,
    {
        When(self, condition)
    }

    fn when_name(
        self,
        models: impl Fn(&str) -> bool,
    ) -> When<Self, impl Fn(&OpenRouterRequest) -> bool>
    where
        Self: Sized,
    {
        self.when(move |r| r.model.as_ref().is_some_and(|m| models(m.as_str())))
    }
}

pub struct When<T, F>(T, F);

impl<T: Transformer, F: Fn(&OpenRouterRequest) -> bool> Transformer for When<T, F> {
    fn transform(&self, request: OpenRouterRequest) -> OpenRouterRequest {
        if (self.1)(&request) {
            self.0.transform(request)
        } else {
            request
        }
    }
}

struct Identity;

impl Transformer for Identity {
    fn transform(&self, request: OpenRouterRequest) -> OpenRouterRequest {
        request
    }
}

pub struct Combine<A, B>(A, B);

impl<A: Transformer, B: Transformer> Transformer for Combine<A, B> {
    fn transform(&self, request: OpenRouterRequest) -> OpenRouterRequest {
        self.0.transform(self.1.transform(request))
    }
}

pub fn pipeline() -> impl Transformer {
    Identity
        .combine(DropToolCalls.when_name(|name| name.contains("mistral")))
        .combine(SetToolChoice::new(ToolChoice::Auto).when_name(|name| name.contains("gemini")))
        .combine(SetCache.when_name(|name| {
            ["mistral", "gemini", "gpt"]
                .iter()
                .all(|p| !name.contains(p))
        }))
}
