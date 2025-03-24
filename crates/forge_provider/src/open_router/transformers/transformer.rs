use regex::Regex;

use super::combine::Combine;
use super::when::When;
use crate::open_router::request::OpenRouterRequest;

/// A trait for transforming OpenRouterRequest based on model-specific
/// requirements
pub trait Transformer {
    /// Transform a request based on a regex pattern matching the model name
    /// The `matches` parameter when set to true will apply the transformation
    /// if the model matches the pattern and vice-versa if set to false.
    fn when_model_matches_condition(
        self,
        pattern: &str,
        matches: bool,
    ) -> When<Self, impl Fn(&OpenRouterRequest) -> bool>
    where
        Self: Sized,
    {
        let regex =
            Regex::new(pattern).unwrap_or_else(|_| panic!("Invalid regex pattern {}", pattern));

        self.when(move |req| {
            req.model
                .as_ref()
                .map(|name| regex.is_match(name.as_str()) && matches)
                .unwrap_or(true)
        })
    }

    fn when_model(self, pattern: &str) -> When<Self, impl Fn(&OpenRouterRequest) -> bool>
    where
        Self: Sized,
    {
        self.when_model_matches_condition(pattern, true)
    }

    fn except_when_model(self, pattern: &str) -> When<Self, impl Fn(&OpenRouterRequest) -> bool>
    where
        Self: Sized,
    {
        self.when_model_matches_condition(pattern, false)
    }

    /// Transform the request according to the transformer's logic
    fn transform(&self, request: OpenRouterRequest) -> OpenRouterRequest;

    /// Combines this transformer with another, creating a new transformer that
    /// applies both transformations in sequence
    fn combine<Other>(self, other: Other) -> Combine<Self, Other>
    where
        Self: Sized,
    {
        Combine(self, other)
    }

    /// Creates a conditional transformer that only applies the transformation
    /// if the given condition is true
    fn when<F: Fn(&OpenRouterRequest) -> bool>(self, condition: F) -> When<Self, F>
    where
        Self: Sized,
    {
        When(self, condition)
    }
}
