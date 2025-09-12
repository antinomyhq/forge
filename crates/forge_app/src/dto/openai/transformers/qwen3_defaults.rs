use forge_domain::Transformer;

use crate::dto::openai::Request;

/// Transformer that applies optimal default parameters for Qwen3 models
///
/// This transformer follows a smart approach:
/// 1. Sets parameters that are not already configured anywhere in the configuration hierarchy
/// 2. Overrides generic default values with Qwen3-optimized values when the user hasn't
///    specifically configured them
///
/// Applies:
/// - temperature: 0.7 (if None)
/// - repetition_penalty: 1.05 (if None)
/// - top_k: 20 (if generic default of 30, only for Qwen models)
/// - max_tokens: 65536 (if generic default of 20480, only for Qwen models)
///
/// Respects all user-specific configurations at any level and only overrides
/// the generic defaults from forge.default.yaml when appropriate.
pub struct Qwen3DefaultParameters;

impl Qwen3DefaultParameters {
    pub fn new() -> Self {
        Self
    }

    /// Checks if the request is for a Qwen model
    fn is_qwen_model(request: &Request) -> bool {
        request
            .model
            .as_ref()
            .map(|model_id| {
                let model_name = model_id.as_str().to_lowercase();
                model_name.contains("qwen")
            })
            .unwrap_or(false)
    }
}

impl Default for Qwen3DefaultParameters {
    fn default() -> Self {
        Self::new()
    }
}

impl Transformer for Qwen3DefaultParameters {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        // Only apply to Qwen models
        if !Self::is_qwen_model(&request) {
            return request;
        }

        // Set parameters that are truly unset (None)
        // This respects all user configurations from YAML, agent, workflow levels

        if request.temperature.is_none() {
            request.temperature = Some(0.7);
        }

        if request.repetition_penalty.is_none() {
            request.repetition_penalty = Some(1.05);
        }

        // Override generic defaults with Qwen3-specific values ONLY when:
        // 1. The model is Qwen
        // 2. The current value matches the generic default from forge.default.yaml
        // 3. User has not specifically set a different value (not the default)
        
        // Override top_k from generic default of 30 to Qwen3-optimal 20
        if request.top_k == Some(30) {
            request.top_k = Some(20);
        }

        // Override max_tokens from generic default of 20480 to Qwen3-optimal 65536
        if request.max_tokens == Some(20480) {
            request.max_tokens = Some(65536);
        }

        // Note: We intentionally do NOT override top_p as 0.8 is already optimal for Qwen3
        // and we respect all user configurations.

        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{ModelId, Transformer};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::dto::openai::Request;

    fn qwen3_request() -> Request {
        Request::default().model(ModelId::new("qwen/qwen3-235b-a22b"))
    }

    fn non_qwen_request() -> Request {
        Request::default().model(ModelId::new("openai/gpt-4"))
    }

    #[test]
    fn test_qwen3_model_detection() {
        let fixture = qwen3_request();
        assert!(Qwen3DefaultParameters::is_qwen_model(&fixture));
    }

    #[test]
    fn test_non_qwen_model_detection() {
        let fixture = non_qwen_request();
        assert!(!Qwen3DefaultParameters::is_qwen_model(&fixture));
    }

    #[test]
    fn test_qwen_model_detection_case_insensitive() {
        let fixture = Request::default().model(ModelId::new("provider/QWEN-turbo"));
        assert!(Qwen3DefaultParameters::is_qwen_model(&fixture));
    }

    #[test]
    fn test_no_model_detection() {
        let fixture = Request::default(); // No model set
        assert!(!Qwen3DefaultParameters::is_qwen_model(&fixture));
    }

    #[test]
    fn test_applies_defaults_to_qwen_model_when_none() {
        // Fixture: Qwen model with no parameters set
        let fixture = qwen3_request();

        // Execute
        let mut transformer = Qwen3DefaultParameters::new();
        let actual = transformer.transform(fixture);

        // Expected: Qwen3 defaults applied
        assert_eq!(actual.temperature, Some(0.7));
        assert_eq!(actual.repetition_penalty, Some(1.05));
    }

    #[test]
    fn test_respects_existing_temperature_configuration() {
        // Fixture: Qwen model with temperature already configured
        let fixture = qwen3_request().temperature(0.5);

        // Execute
        let mut transformer = Qwen3DefaultParameters::new();
        let actual = transformer.transform(fixture);

        // Expected: Existing temperature preserved, only repetition_penalty added
        assert_eq!(actual.temperature, Some(0.5)); // User's configuration preserved
        assert_eq!(actual.repetition_penalty, Some(1.05)); // Default applied
    }

    #[test]
    fn test_respects_existing_repetition_penalty_configuration() {
        // Fixture: Qwen model with repetition_penalty already configured
        let fixture = qwen3_request().repetition_penalty(1.2);

        // Execute
        let mut transformer = Qwen3DefaultParameters::new();
        let actual = transformer.transform(fixture);

        // Expected: Existing repetition_penalty preserved, only temperature added
        assert_eq!(actual.temperature, Some(0.7)); // Default applied
        assert_eq!(actual.repetition_penalty, Some(1.2)); // User's configuration preserved
    }

    #[test]
    fn test_respects_all_existing_configurations() {
        // Fixture: Qwen model with both parameters already configured
        let fixture = qwen3_request().temperature(0.5).repetition_penalty(1.2);

        // Execute
        let mut transformer = Qwen3DefaultParameters::new();
        let actual = transformer.transform(fixture);

        // Expected: All existing configurations preserved
        assert_eq!(actual.temperature, Some(0.5));
        assert_eq!(actual.repetition_penalty, Some(1.2));
    }

    #[test]
    fn test_does_not_affect_non_qwen_models() {
        // Fixture: Non-Qwen model with no parameters set
        let fixture = non_qwen_request();

        // Execute
        let mut transformer = Qwen3DefaultParameters::new();
        let actual = transformer.transform(fixture);

        // Expected: No changes applied
        assert_eq!(actual.temperature, None);
        assert_eq!(actual.repetition_penalty, None);
    }

    #[test]
    fn test_overrides_generic_defaults_for_qwen_models() {
        // Fixture: Qwen model with generic default parameters from forge.default.yaml
        let fixture = qwen3_request()
            .top_k(30) // Generic default from YAML
            .max_tokens(20480); // Generic default from YAML

        // Execute
        let mut transformer = Qwen3DefaultParameters::new();
        let actual = transformer.transform(fixture);

        // Expected: Generic defaults overridden with Qwen3-optimized values
        assert_eq!(actual.top_k, Some(20)); // Overridden to Qwen3-optimal
        assert_eq!(actual.max_tokens, Some(65536)); // Overridden to Qwen3-optimal
        assert_eq!(actual.temperature, Some(0.7)); // Default applied (was None)
        assert_eq!(actual.repetition_penalty, Some(1.05)); // Default applied (was None)
    }

    #[test]
    fn test_respects_user_configured_values_for_qwen_models() {
        // Fixture: Qwen model with user-configured parameters (not generic defaults)
        let fixture = qwen3_request()
            .top_k(50) // User-configured value (not generic default)
            .max_tokens(30000); // User-configured value (not generic default)

        // Execute
        let mut transformer = Qwen3DefaultParameters::new();
        let actual = transformer.transform(fixture);

        // Expected: User-configured values preserved
        assert_eq!(actual.top_k, Some(50)); // User config preserved
        assert_eq!(actual.max_tokens, Some(30000)); // User config preserved
        assert_eq!(actual.temperature, Some(0.7)); // Default applied (was None)
        assert_eq!(actual.repetition_penalty, Some(1.05)); // Default applied (was None)
    }

    #[test]
    fn test_respects_top_p_from_yaml() {
        // Fixture: Qwen model with top_p parameter from YAML
        let fixture = qwen3_request()
            .top_p(0.8); // From YAML (already optimal for Qwen3)

        // Execute
        let mut transformer = Qwen3DefaultParameters::new();
        let actual = transformer.transform(fixture);

        // Expected: top_p preserved, other defaults applied
        assert_eq!(actual.top_p, Some(0.8)); // YAML config preserved
        assert_eq!(actual.temperature, Some(0.7)); // Default applied (was None)
        assert_eq!(actual.repetition_penalty, Some(1.05)); // Default applied (was None)
    }

    #[test]
    fn test_various_qwen_model_formats() {
        let qwen_models = vec![
            "qwen/qwen3-235b-a22b",
            "provider/qwen-turbo",
            "qwen2.5",
            "alibaba/qwen3-instruct",
        ];

        for model_name in qwen_models {
            let fixture = Request::default().model(ModelId::new(model_name));

            let mut transformer = Qwen3DefaultParameters::new();
            let actual = transformer.transform(fixture);

            // All should get Qwen3 defaults applied
            assert_eq!(actual.temperature, Some(0.7), "Model: {model_name}");
            assert_eq!(actual.repetition_penalty, Some(1.05), "Model: {model_name}");
        }
    }
}
