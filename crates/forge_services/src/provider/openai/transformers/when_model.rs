use regex::Regex;

use super::super::request::Request;

/// Creates a condition function that matches requests when the model name
/// matches the given regex pattern.
///
/// # Arguments
/// * `pattern` - A regex pattern to match against the model name
///
/// # Returns
/// A function that returns true when the model name matches the pattern.
///
/// # Examples
/// ```rust,ignore
/// // Apply transformation only for Claude models
/// let conditional_transformer = my_transformer.when(when_model("claude"));
/// ```
pub fn when_model(pattern: &str) -> impl Fn(&Request) -> bool {
    let regex = Regex::new(pattern).unwrap_or_else(|_| panic!("Invalid regex pattern: {pattern}"));

    move |req: &Request| {
        req.model
            .as_ref()
            .map(|name| regex.is_match(name.as_str()))
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use forge_app::domain::{ModelId, Transformer};
    use pretty_assertions::assert_eq;

    use super::super::super::request::Request;
    use super::*;

    // A simple test transformer that adds a prefix to the model name
    struct TestTransformer {
        prefix: String,
    }

    impl Transformer for TestTransformer {
        type Value = Request;

        fn transform(&mut self, mut request: Self::Value) -> Self::Value {
            if let Some(model) = request.model.as_mut() {
                let new_model = format!("{}{}", self.prefix, model.as_str());
                *model = ModelId::new(&new_model);
            }
            request
        }
    }

    #[test]
    fn test_when_model_matches() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };
        let request = Request::default().model(ModelId::new("anthropic/claude-3"));

        // Apply transformation with condition that should match
        let condition = when_model("claude");
        let mut conditional = transformer.when(condition);
        let actual = conditional.transform(request);

        // Expected: model name should be prefixed
        assert_eq!(actual.model.unwrap().as_str(), "prefix-anthropic/claude-3");
    }

    #[test]
    fn test_when_model_no_match() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };
        let request = Request::default().model(ModelId::new("openai/gpt-4"));

        // Apply transformation with condition that should not match
        let condition = when_model("claude");
        let mut conditional = transformer.when(condition);
        let actual = conditional.transform(request);

        // Expected: model name should remain unchanged
        assert_eq!(actual.model.unwrap().as_str(), "openai/gpt-4");
    }

    #[test]
    fn test_when_model_no_model() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };
        let request = Request::default(); // No model set

        // Apply transformation with when_model
        let condition = when_model("claude");
        let mut conditional = transformer.when(condition);
        let actual = conditional.transform(request);

        // Expected: request should remain unchanged
        assert!(actual.model.is_none());
    }

    #[test]
    #[should_panic(expected = "Invalid regex pattern")]
    fn test_when_model_invalid_regex() {
        // This test should panic due to invalid regex
        let _condition = when_model("[invalid");
    }

    #[test]
    fn test_complex_regex_patterns() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };

        // Test with complex regex pattern
        let request = Request::default().model(ModelId::new("anthropic/claude-3-sonnet"));
        let condition = when_model("claude-[0-9]+-sonnet");
        let mut conditional = transformer.when(condition);
        let actual = conditional.transform(request);

        // Expected: model name should be prefixed
        assert_eq!(
            actual.model.unwrap().as_str(),
            "prefix-anthropic/claude-3-sonnet"
        );
    }

    #[test]
    fn test_case_sensitive_matching() {
        // Fixture
        let transformer = TestTransformer { prefix: "prefix-".to_string() };

        // Test case sensitivity
        let request = Request::default().model(ModelId::new("anthropic/Claude-3"));
        let condition = when_model("claude"); // lowercase
        let mut conditional = transformer.when(condition);
        let actual = conditional.transform(request);

        // Expected: model name should remain unchanged (case mismatch)
        assert_eq!(actual.model.unwrap().as_str(), "anthropic/Claude-3");
    }
}
