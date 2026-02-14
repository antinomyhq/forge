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

        // Round-robin strategy: change effort every 10 messages
        // Cycle: Minimal -> Low -> Medium -> High -> Xhigh
        let step = assistant_msg_count / 15;
        let level_index = step % 5;

        match level_index {
            0 => oai::ReasoningEffort::Minimal,
            1 => oai::ReasoningEffort::Low,
            2 => oai::ReasoningEffort::Medium,
            3 => oai::ReasoningEffort::High,
            4 => oai::ReasoningEffort::Xhigh,
            _ => oai::ReasoningEffort::Medium, // Unreachable with % 5
        }
    }
}

impl Transformer for CodexTransformer {
    type Value = CreateResponse;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        request.store = Some(false);
        request.temperature = None;
        request.max_output_tokens = None;

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

    struct Fixture {
        reasoning: Option<oai::Reasoning>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                reasoning: Some(oai::Reasoning {
                    effort: Some(oai::ReasoningEffort::Medium),
                    summary: Some(oai::ReasoningSummary::Detailed),
                }),
            }
        }

        fn create_request(&self, assistant_msg_count: usize) -> CreateResponse {
            let mut context = forge_app::domain::Context::default()
                .max_tokens(1024usize)
                .temperature(forge_app::domain::Temperature::from(0.7));

            // Add initial user message
            context = context.add_message(ContextMessage::user("Hello", None));

            for i in 0..assistant_msg_count {
                context = context
                    .add_message(ContextMessage::assistant(format!("A{}", i), None, None, None))
                    .add_message(ContextMessage::user(format!("Q{}", i), None));
            }

            let mut req = oai::CreateResponse::from_domain(context).unwrap();
            req.model = Some("gpt-5.1-codex".to_string());
            req.reasoning = self.reasoning.clone();
            req
        }
    }

    #[test]
    fn test_codex_transformer_sets_store_false() {
        let fixture = Fixture::new();
        let mut request = fixture.create_request(0);
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(request);

        assert_eq!(actual.store, Some(false));
    }

    #[test]
    fn test_codex_transformer_strips_temperature() {
        let fixture = Fixture::new();
        let mut request = fixture.create_request(0);
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(request);

        assert_eq!(actual.temperature, None);
    }

    #[test]
    fn test_codex_transformer_strips_max_output_tokens() {
        let fixture = Fixture::new();
        let mut request = fixture.create_request(0);
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(request);

        assert_eq!(actual.max_output_tokens, None);
    }

    #[test]
    fn test_codex_transformer_includes_reasoning_encrypted_content() {
        let fixture = Fixture::new();
        let mut request = fixture.create_request(0);
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(request);

        let expected = vec![
            oai::IncludeEnum::WebSearchCallActionSources,
            oai::IncludeEnum::CodeInterpreterCallOutputs,
            oai::IncludeEnum::ReasoningEncryptedContent,
        ];
        assert_eq!(actual.include, Some(expected));
    }

    #[test]
    fn test_codex_transformer_round_robin_cycle() {
        let fixture = Fixture::new();
        let mut transformer = CodexTransformer;
        
        // 0-9 messages -> Minimal
        let request = fixture.create_request(0);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Minimal)
        );

        let request = fixture.create_request(9);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Minimal)
        );

        // 10-19 messages -> Low
        let request = fixture.create_request(10);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Low)
        );

        let request = fixture.create_request(19);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Low)
        );

        // 20-29 messages -> Medium
        let request = fixture.create_request(20);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Medium)
        );

        let request = fixture.create_request(29);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Medium)
        );

        // 30-39 messages -> High
        let request = fixture.create_request(30);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::High)
        );

        let request = fixture.create_request(39);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::High)
        );

        // 40-49 messages -> XHigh
        let request = fixture.create_request(40);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Xhigh)
        );

        let request = fixture.create_request(49);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Xhigh)
        );

        // 50-59 messages -> Minimal (cycle repeats)
        let request = fixture.create_request(50);
        let request = transformer.transform(request);
        assert_eq!(
            request.reasoning.as_ref().unwrap().effort,
            Some(oai::ReasoningEffort::Minimal)
        );
    }

    #[test]
    fn test_codex_transformer_sets_text_verbosity_low() {
        let fixture = Fixture::new();
        let mut request = fixture.create_request(0);
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(request);

        let expected = Some(oai::Verbosity::Low);
        assert_eq!(
            actual.text.as_ref().and_then(|t| t.verbosity.clone()),
            expected
        );
    }

    #[test]
    fn test_codex_transformer_no_reasoning_unchanged() {
        let fixture = Fixture::new();
        let mut request = fixture.create_request(0);
        request.reasoning = None;
        let mut transformer = CodexTransformer;
        let actual = transformer.transform(request);

        assert_eq!(actual.reasoning, None);
    }
}
