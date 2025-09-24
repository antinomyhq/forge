use forge_domain::{DefaultTransformation, Provider, Transformer};

use super::drop_tool_call::DropToolCalls;
use super::make_cerebras_compat::MakeCerebrasCompat;
use super::make_openai_compat::MakeOpenAiCompat;
use super::set_cache::SetCache;
use super::set_provider_preferences::SetProviderPreferences;
use super::set_temperature::SetTemperature;
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
            .pipe(
                SetProviderPreferences::new(
                    vec!["moonshotai".to_string(), "groq".to_string()],
                    false,
                )
                .when(when_model("kimi-k2")),
            )
            .when(move |_| supports_open_router_params(provider));

        let open_ai_compat = MakeOpenAiCompat.when(move |_| !supports_open_router_params(provider));

        let cerebras_compat = MakeCerebrasCompat.when(move |_| provider.is_cerebras());

        let mut combined = or_transformers
            .pipe(open_ai_compat)
            .pipe(cerebras_compat)
            .pipe(SetTemperature::new(0.6).when(when_model("kimi-k2")));
        combined.transform(request)
    }
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider) -> bool {
    provider.is_open_router()
        || provider.is_forge()
        || provider.is_zai()
        || provider.is_zai_coding()
}

#[cfg(test)]
mod tests {
    use forge_domain::ModelId;
    use pretty_assertions::assert_eq;

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

    #[test]
    fn test_kimi_k2_temperature_set_for_open_router() {
        // Fixture
        let provider = Provider::open_router("openrouter");
        let mut pipeline = ProviderPipeline::new(&provider);
        let request = Request::default().model(ModelId::new("kimi-k2-128k"));

        // Execute
        let actual = pipeline.transform(request);

        // Expected: temperature should be set to 0.6 for kimi-k2 models
        assert_eq!(actual.temperature, Some(0.6));
    }

    #[test]
    fn test_non_kimi_model_temperature_unchanged() {
        // Fixture
        let provider = Provider::open_router("openrouter");
        let mut pipeline = ProviderPipeline::new(&provider);
        let request = Request::default()
            .model(ModelId::new("gpt-4"))
            .temperature(1.0);

        // Execute
        let actual = pipeline.transform(request);

        // Expected: temperature should remain unchanged for non-kimi models
        assert_eq!(actual.temperature, Some(1.0));
    }

    #[test]
    fn test_kimi_k2_provider_preferences_set_for_open_router() {
        // Fixture
        let provider = Provider::open_router("openrouter");
        let mut pipeline = ProviderPipeline::new(&provider);
        let request = Request::default().model(ModelId::new("kimi-k2-128k"));

        // Execute
        let actual = pipeline.transform(request);

        // Expected: provider preferences should be set for kimi-k2 models
        let expected_preferences = Some(crate::dto::openai::ProviderPreferences {
            order: vec!["moonshotai".to_string(), "groq".to_string()],
            allow_fallbacks: false,
        });
        assert_eq!(actual.provider, expected_preferences);
    }

    #[test]
    fn test_non_kimi_model_provider_preferences_unchanged() {
        // Fixture
        let provider = Provider::open_router("openrouter");
        let mut pipeline = ProviderPipeline::new(&provider);
        let request = Request::default().model(ModelId::new("gpt-4"));

        // Execute
        let actual = pipeline.transform(request);

        // Expected: provider preferences should remain None for non-kimi models
        assert_eq!(actual.provider, None);
    }

    #[test]
    fn test_kimi_k2_provider_preferences_not_set_for_non_openrouter() {
        // Fixture
        let provider = Provider::openai("openai");
        let mut pipeline = ProviderPipeline::new(&provider);
        let request = Request::default().model(ModelId::new("kimi-k2-128k"));

        // Execute
        let actual = pipeline.transform(request);

        // Expected: provider preferences should not be set for non-OpenRouter providers
        assert_eq!(actual.provider, None);
    }
}
