use forge_app::domain::{Context, Transformer};

pub struct ReasoningTransform;

impl Transformer for ReasoningTransform {
    type Value = Context;
    fn transform(&mut self, mut context: Self::Value) -> Self::Value {
        if let Some(reasoning) = context.reasoning.as_ref()
            && reasoning.enabled.unwrap_or(false)
            && reasoning.max_tokens.is_some()
        {
            // if reasoning is enabled then we've to drop top_k and top_p
            context.top_k = None;
            context.top_p = None;
        }

        context
    }
}

#[cfg(test)]
mod tests {
    use forge_app::domain::{Context, ReasoningConfig, TopK, TopP, Transformer};
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_context_fixture() -> Context {
        Context::default()
            .top_k(TopK::new(50).unwrap())
            .top_p(TopP::new(0.8).unwrap())
    }

    fn create_reasoning_config_fixture(
        enabled: bool,
        max_tokens: Option<usize>,
    ) -> ReasoningConfig {
        ReasoningConfig {
            enabled: Some(enabled),
            max_tokens,
            effort: None,
            exclude: None,
        }
    }

    #[test]
    fn test_reasoning_enabled_with_max_tokens_removes_top_k_and_top_p() {
        let fixture =
            create_context_fixture().reasoning(create_reasoning_config_fixture(true, Some(1024)));
        let mut transformer = ReasoningTransform;
        let actual = transformer.transform(fixture);
        let expected =
            Context::default().reasoning(create_reasoning_config_fixture(true, Some(1024)));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_reasoning_disabled_preserves_top_k_and_top_p() {
        let fixture =
            create_context_fixture().reasoning(create_reasoning_config_fixture(false, Some(1024)));
        let mut transformer = ReasoningTransform;
        let actual = transformer.transform(fixture.clone());
        let expected = fixture;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_reasoning_enabled_without_max_tokens_preserves_top_k_and_top_p() {
        let fixture =
            create_context_fixture().reasoning(create_reasoning_config_fixture(true, None));
        let mut transformer = ReasoningTransform;
        let actual = transformer.transform(fixture.clone());
        let expected = fixture;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_reasoning_config_preserves_top_k_and_top_p() {
        let fixture = create_context_fixture();
        let mut transformer = ReasoningTransform;
        let actual = transformer.transform(fixture.clone());
        let expected = fixture;

        assert_eq!(actual, expected);
    }
}
