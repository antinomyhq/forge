use async_openai::types::responses::{self as oai, CreateResponse};
use forge_domain::Transformer;

/// Transformer that adjusts Responses API requests for the Codex backend.
///
/// The Codex backend at `chatgpt.com/backend-api/codex/responses` differs from
/// the standard OpenAI Responses API in several ways:
/// - `store` **must** be `false` (the server defaults to `true` and rejects
///   omitted values).
/// - `temperature` is not supported and must be stripped.
/// - `max_output_tokens` is not supported and must be stripped.
/// - `include` always contains `reasoning.encrypted_content` for stateless
///   reasoning continuity.
/// - `text.verbosity` is forced to `Low` for concise output.
/// - `reasoning.effort` is forced to `High` and `reasoning.summary` to `Auto`.
pub struct CodexTransformer;

impl CodexTransformer {
    fn determine_effort(request: &CreateResponse) -> oai::ReasoningEffort {
        let items = match &request.input {
            oai::InputParam::Items(items) => items,
            _ => return oai::ReasoningEffort::Medium,
        };

        let assistant_msg_count = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    oai::InputItem::EasyMessage(msg) if msg.role == oai::Role::Assistant
                )
            })
            .count();

        if assistant_msg_count >= 80 {
            oai::ReasoningEffort::Xhigh
        } else if assistant_msg_count >= 50 {
            oai::ReasoningEffort::High
        } else if assistant_msg_count >= 15 {
            oai::ReasoningEffort::Medium
        } else {
            oai::ReasoningEffort::Low
        }
    }
}

impl Transformer for CodexTransformer {
    type Value = CreateResponse;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        request.store = Some(false);
        request.temperature = Some(0.2);
        request.max_output_tokens = None;
        request.top_p = None;

        let includes = request.include.get_or_insert_with(Vec::new);
        if !includes.contains(&oai::IncludeEnum::ReasoningEncryptedContent) {
            includes.push(oai::IncludeEnum::ReasoningEncryptedContent);
        }

        // Force text verbosity to Low for concise codex output
        let text = request.text.get_or_insert(oai::ResponseTextParam {
            format: oai::TextResponseFormatConfiguration::Text,
            verbosity: None,
        });
        text.verbosity = Some(oai::Verbosity::Low);

        let effort = Self::determine_effort(&request);

        if let Some(reasoning) = request.reasoning.as_mut() {
            reasoning.effort = Some(effort);
            reasoning.summary = Some(oai::ReasoningSummary::Concise);
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use async_openai::types::responses as oai;
    use forge_app::domain::ContextMessage;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::provider::FromDomain;

    fn fixture() -> CreateResponse {
        let context = forge_app::domain::Context::default()
            .add_message(ContextMessage::user("Hello", None))
            .max_tokens(1024usize)
            .temperature(forge_app::domain::Temperature::from(0.7));

        let mut req = oai::CreateResponse::from_domain(context).unwrap();
        req.model = Some("gpt-5.1-codex".to_string());
        req
    }

    #[test]
    fn test_codex_transformer_sets_store_false() {
        let fixture = fixture();
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.store, Some(false));
    }

    #[test]
    fn test_codex_transformer_strips_temperature() {
        let fixture = fixture();
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.temperature, None);
    }

    #[test]
    fn test_codex_transformer_strips_max_output_tokens() {
        let fixture = fixture();
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.max_output_tokens, None);
    }

    #[test]
    fn test_codex_transformer_includes_reasoning_encrypted_content() {
        let fixture = fixture();
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        let expected = vec![oai::IncludeEnum::ReasoningEncryptedContent];
        assert_eq!(actual.include, Some(expected));
    }

    #[test]
    fn test_codex_transformer_preserves_existing_includes_and_appends_reasoning_encrypted_content()
    {
        let mut fixture = fixture();
        fixture.include = Some(vec![oai::IncludeEnum::MessageOutputTextLogprobs]);
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        let expected = vec![
            oai::IncludeEnum::MessageOutputTextLogprobs,
            oai::IncludeEnum::ReasoningEncryptedContent,
        ];
        assert_eq!(actual.include, Some(expected));
    }

    #[test]
    fn test_codex_transformer_does_not_duplicate_reasoning_encrypted_content_include() {
        let mut fixture = fixture();
        fixture.include = Some(vec![oai::IncludeEnum::ReasoningEncryptedContent]);
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        let expected = vec![oai::IncludeEnum::ReasoningEncryptedContent];
        assert_eq!(actual.include, Some(expected));
    }

    #[test]
    fn test_codex_transformer_sets_reasoning_effort_low_initially() {
        let reasoning = oai::Reasoning {
            effort: Some(oai::ReasoningEffort::Medium),
            summary: Some(oai::ReasoningSummary::Detailed),
        };

        let mut fixture = fixture();
        fixture.reasoning = Some(reasoning);
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(
            actual.reasoning.as_ref().and_then(|r| r.effort.clone()),
            Some(oai::ReasoningEffort::Low)
        );
    }

    #[test]
    fn test_codex_transformer_sets_reasoning_effort_medium_after_10_messages() {
        let reasoning = oai::Reasoning {
            effort: Some(oai::ReasoningEffort::Low),
            summary: Some(oai::ReasoningSummary::Detailed),
        };

        // Create context with 10 assistant messages
        let mut context = forge_app::domain::Context::default();
        for i in 0..10 {
            context = context
                .add_message(ContextMessage::user(format!("Q{}", i), None))
                .add_message(ContextMessage::assistant(
                    format!("A{}", i),
                    None,
                    None,
                    None,
                ));
        }

        let mut fixture = oai::CreateResponse::from_domain(context).unwrap();
        fixture.reasoning = Some(reasoning);

        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(
            actual.reasoning.as_ref().and_then(|r| r.effort.clone()),
            Some(oai::ReasoningEffort::Medium)
        );
    }

    #[test]
    fn test_codex_transformer_sets_reasoning_effort_high_after_20_messages() {
        let reasoning = oai::Reasoning {
            effort: Some(oai::ReasoningEffort::Low),
            summary: Some(oai::ReasoningSummary::Detailed),
        };

        // Create context with 20 assistant messages
        let mut context = forge_app::domain::Context::default();
        for i in 0..20 {
            context = context
                .add_message(ContextMessage::user(format!("Q{}", i), None))
                .add_message(ContextMessage::assistant(
                    format!("A{}", i),
                    None,
                    None,
                    None,
                ));
        }

        let mut fixture = oai::CreateResponse::from_domain(context).unwrap();
        fixture.reasoning = Some(reasoning);

        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(
            actual.reasoning.as_ref().and_then(|r| r.effort.clone()),
            Some(oai::ReasoningEffort::High)
        );
    }

    #[test]
    fn test_codex_transformer_sets_reasoning_effort_xhigh_after_50_messages() {
        let reasoning = oai::Reasoning {
            effort: Some(oai::ReasoningEffort::Low),
            summary: Some(oai::ReasoningSummary::Detailed),
        };

        // Create context with 50 assistant messages
        let mut context = forge_app::domain::Context::default();
        for i in 0..50 {
            context = context
                .add_message(ContextMessage::user(format!("Q{}", i), None))
                .add_message(ContextMessage::assistant(
                    format!("A{}", i),
                    None,
                    None,
                    None,
                ));
        }

        let mut fixture = oai::CreateResponse::from_domain(context).unwrap();
        fixture.reasoning = Some(reasoning);

        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(
            actual.reasoning.as_ref().and_then(|r| r.effort.clone()),
            Some(oai::ReasoningEffort::Xhigh)
        );
    }

    #[test]
    fn test_codex_transformer_sets_text_verbosity_low() {
        let fixture = fixture();
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        let expected = Some(oai::Verbosity::Low);
        assert_eq!(
            actual.text.as_ref().and_then(|t| t.verbosity.clone()),
            expected
        );
    }

    #[test]
    fn test_codex_transformer_overrides_text_verbosity_to_low() {
        let mut fixture = fixture();
        fixture.text = Some(oai::ResponseTextParam {
            format: oai::TextResponseFormatConfiguration::Text,
            verbosity: Some(oai::Verbosity::High),
        });
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        let expected = Some(oai::Verbosity::Low);
        assert_eq!(
            actual.text.as_ref().and_then(|t| t.verbosity.clone()),
            expected
        );
    }

    #[test]
    fn test_codex_transformer_no_reasoning_unchanged() {
        let fixture = fixture();
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.reasoning, None);
    }

    #[test]
    fn test_codex_transformer_preserves_other_fields() {
        let fixture = fixture();
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.model.as_deref(), Some("gpt-5.1-codex"));
        assert_eq!(actual.stream, Some(true));
    }
}
