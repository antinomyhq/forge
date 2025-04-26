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
            Regex::new(pattern).unwrap_or_else(|_| panic!("Invalid regex pattern {pattern}"));

        self.when(move |req| {
            req.model
                .as_ref()
                .map(|name| regex.is_match(name.as_str()) == matches)
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

#[cfg(test)]
mod tests {
    use forge_domain::ModelId;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::open_router::transformers::identity::Identity;

    // A simple test transformer that adds a prefix to the model name
    struct TestTransformer {
        prefix: String,
    }

    impl Transformer for TestTransformer {
        fn transform(&self, mut request: OpenRouterRequest) -> OpenRouterRequest {
            if let Some(model) = request.model.as_mut() {
                let new_model = format!("{}{}", self.prefix, model.as_str());
                *model = ModelId::new(&new_model);
            }
            request
        }
    }

    #[test]
    fn test_when_model_matches_condition_true() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };
        let request = OpenRouterRequest::default().model(ModelId::new("anthropic/claude-3"));

        // Apply transformation with condition that should match
        let conditional = transformer.when_model_matches_condition("claude", true);
        let actual = conditional.transform(request);

        // Expected: model name should be prefixed
        assert_eq!(actual.model.unwrap().as_str(), "prefix-anthropic/claude-3");
    }

    #[test]
    fn test_when_model_matches_condition_false() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };
        let request = OpenRouterRequest::default().model(ModelId::new("anthropic/claude-3"));

        // Apply transformation with condition that should not match
        let conditional = transformer.when_model_matches_condition("claude", false);
        let actual = conditional.transform(request);

        // Expected: model name should remain unchanged
        assert_eq!(actual.model.unwrap().as_str(), "anthropic/claude-3");
    }

    #[test]
    fn test_when_model_matches_condition_no_match() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };
        let request = OpenRouterRequest::default().model(ModelId::new("openai/gpt-4"));

        // Apply transformation with condition that should not match
        let conditional = transformer.when_model_matches_condition("claude", true);
        let actual = conditional.transform(request);

        // Expected: model name should remain unchanged
        assert_eq!(actual.model.unwrap().as_str(), "openai/gpt-4");
    }

    #[test]
    fn test_when_model() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };
        let request = OpenRouterRequest::default().model(ModelId::new("anthropic/claude-3"));

        // Apply transformation with when_model
        let conditional = transformer.when_model("claude");
        let actual = conditional.transform(request);

        // Expected: model name should be prefixed
        assert_eq!(actual.model.unwrap().as_str(), "prefix-anthropic/claude-3");
    }

    #[test]
    fn test_except_when_model() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };

        // Test with a model that should be excluded
        let request1 = OpenRouterRequest::default().model(ModelId::new("anthropic/claude-3"));
        let conditional = transformer.except_when_model("claude");
        let actual1 = conditional.transform(request1);
        // Expected: model name should remain unchanged (because it matches the pattern
        // and matches=false)
        assert_eq!(actual1.model.unwrap().as_str(), "anthropic/claude-3");

        // Create a new transformer since the previous one was consumed
        let transformer2 = TestTransformer { prefix: "prefix-".to_string() };
        // Test with a model that should not be excluded
        let request2 = OpenRouterRequest::default().model(ModelId::new("openai/gpt-4"));
        let conditional2 = transformer2.except_when_model("claude");
        let actual2 = conditional2.transform(request2);
        // Expected: model name should be prefixed (because it doesn't match the
        // pattern)
        assert_eq!(actual2.model.unwrap().as_str(), "prefix-openai/gpt-4");
    }

    #[test]
    fn test_combine() {
        // Fixture
        let transformer1 = TestTransformer { prefix: "prefix1-".to_string() };
        let transformer2 = TestTransformer { prefix: "prefix2-".to_string() };
        let request = OpenRouterRequest::default().model(ModelId::new("model"));

        // Apply combined transformations
        let combined = transformer1.combine(transformer2);
        let actual = combined.transform(request);

        // Expected: both prefixes should be applied (in the correct order)
        assert_eq!(actual.model.unwrap().as_str(), "prefix1-prefix2-model");
    }

    #[test]
    fn test_when() {
        // Fixture for first test
        let transformer1 = TestTransformer { prefix: "prefix-".to_string() };
        let request1 = OpenRouterRequest::default().model(ModelId::new("model"));

        // Test with a condition that should match
        let conditional1 = transformer1.when(|req| req.model.is_some());
        let actual1 = conditional1.transform(request1);
        // Expected: model name should be prefixed
        assert_eq!(actual1.model.unwrap().as_str(), "prefix-model");

        // Fixture for second test (need a new transformer since when takes ownership)
        let transformer2 = TestTransformer { prefix: "prefix-".to_string() };
        let request2 = OpenRouterRequest::default().model(ModelId::new("model"));

        // Test with a condition that should not match
        let conditional2 = transformer2.when(|req| {
            req.model
                .as_ref()
                .is_some_and(|m| m.as_str().contains("other"))
        });
        let actual2 = conditional2.transform(request2);
        // Expected: model name should remain unchanged
        assert_eq!(actual2.model.unwrap().as_str(), "model");
    }

    #[test]
    fn test_when_model_no_model() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };
        let request = OpenRouterRequest::default(); // No model set

        // Apply transformation with when_model
        let conditional = transformer.when_model("claude");
        let actual = conditional.transform(request);

        // Expected: request should remain unchanged
        assert!(actual.model.is_none());
    }

    #[test]
    fn test_identity_transformer() {
        // Fixture
        let transformer = Identity;
        let request = OpenRouterRequest::default().model(ModelId::new("model"));

        // Apply identity transformation
        let actual = transformer.transform(request.clone());

        // Expected: request should remain unchanged
        assert_eq!(
            actual.model.unwrap().as_str(),
            request.model.unwrap().as_str()
        );
    }
}
