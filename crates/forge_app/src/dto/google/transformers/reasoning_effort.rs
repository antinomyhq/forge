use forge_domain::Transformer;

use crate::dto::google::request::Role;
use crate::dto::google::{Level, Request};

pub struct ReasoningEffort;

impl Transformer for ReasoningEffort {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        let assistant_msg_count = request
            .contents
            .iter()
            .filter(|c| c.role == Some(Role::Model))
            .count();

        let level = if assistant_msg_count < 10 {
            Level::High
        } else if assistant_msg_count < 50 {
            Level::Medium
        } else {
            Level::High
        };

        if let Some(generation_config) = &mut request.generation_config
            && let Some(thinking_config) = &mut generation_config.thinking_config {
                thinking_config.thinking_level = Some(level);
            }

        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Context, ContextMessage, ReasoningConfig};

    use super::*;
    use crate::dto::google::Request;

    fn create_context_with_assistant_messages(count: usize) -> Context {
        let mut context = Context {
            reasoning: Some(ReasoningConfig {
                enabled: Some(true),
                max_tokens: Some(1024),
                ..Default::default()
            }),
            ..Default::default()
        };

        for i in 0..count {
            context = context
                .add_message(ContextMessage::user(format!("Q{}", i), None))
                .add_message(ContextMessage::assistant(
                    format!("A{}", i),
                    None,
                    None,
                    None,
                ));
        }

        context
    }

    #[test]
    fn test_reasoning_effort_high_for_first_10() {
        let context = create_context_with_assistant_messages(5);
        let request = Request::from(context);
        let mut transformer = ReasoningEffort;
        let transformed = transformer.transform(request);

        let thinking_config = transformed
            .generation_config
            .unwrap()
            .thinking_config
            .unwrap();

        assert!(matches!(thinking_config.thinking_level, Some(Level::High)));
    }

    #[test]
    fn test_reasoning_effort_medium_for_10_to_49() {
        let context = create_context_with_assistant_messages(20);
        let request = Request::from(context);
        let mut transformer = ReasoningEffort;
        let transformed = transformer.transform(request);

        let thinking_config = transformed
            .generation_config
            .unwrap()
            .thinking_config
            .unwrap();

        assert!(matches!(
            thinking_config.thinking_level,
            Some(Level::Medium)
        ));
    }

    #[test]
    fn test_reasoning_effort_high_for_50_and_above() {
        let context = create_context_with_assistant_messages(55);
        let request = Request::from(context);
        let mut transformer = ReasoningEffort;
        let transformed = transformer.transform(request);

        let thinking_config = transformed
            .generation_config
            .unwrap()
            .thinking_config
            .unwrap();

        assert!(matches!(thinking_config.thinking_level, Some(Level::High)));
    }
}
