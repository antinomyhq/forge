use super::Transformer;
use crate::{Context, ModelId};

/// Transformer that sets the model for all user messages in the context
pub struct SetModel {
    pub model: ModelId,
}

impl SetModel {
    pub fn new(model: ModelId) -> Self {
        Self { model }
    }
}

impl Transformer for SetModel {
    type Value = Context;

    fn transform(&mut self, mut value: Self::Value) -> Self::Value {
        // Set the model for all user messages that don't already have a model set
        for message in value.messages.iter_mut() {
            if let crate::ContextMessage::Text(text_msg) = message
                && text_msg.role == crate::Role::User
                && text_msg.model.is_none()
            {
                text_msg.model = Some(self.model.clone());
            }
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_yaml_snapshot;
    use pretty_assertions::assert_eq;
    use serde::Serialize;

    use super::*;
    use crate::{ContextMessage, Role, TextMessage};

    #[derive(Serialize)]
    struct TransformationSnapshot {
        transformation: String,
        before: Context,
        after: Context,
    }

    impl TransformationSnapshot {
        fn new(transformation: &str, before: Context, after: Context) -> Self {
            Self { transformation: transformation.to_string(), before, after }
        }
    }

    #[test]
    fn test_set_model_empty_context() {
        let fixture = Context::default();
        let mut transformer = SetModel::new(ModelId::new("gpt-4"));
        let actual = transformer.transform(fixture.clone());
        let expected = fixture;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_set_model_for_user_messages() {
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message 1", None))
            .add_message(ContextMessage::assistant("Assistant response", None, None))
            .add_message(ContextMessage::user("User message 2", None));

        let mut transformer = SetModel::new(ModelId::new("gpt-4"));
        let actual = transformer.transform(fixture.clone());

        let snapshot = TransformationSnapshot::new("SetModel(gpt-4)", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_set_model_preserves_existing_models() {
        let fixture = Context::default()
            .add_message(ContextMessage::user("User message 1", None))
            .add_message(ContextMessage::user(
                "User message 2",
                Some(ModelId::new("claude-3")),
            ))
            .add_message(ContextMessage::user("User message 3", None));

        let mut transformer = SetModel::new(ModelId::new("gpt-4"));
        let actual = transformer.transform(fixture.clone());

        let snapshot =
            TransformationSnapshot::new("SetModel(gpt-4)_preserve_existing", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_set_model_only_affects_user_messages() {
        let fixture = Context::default()
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::System,
                content: "System message".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "Assistant message".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }))
            .add_message(ContextMessage::user("User message", None));

        let mut transformer = SetModel::new(ModelId::new("gpt-4"));
        let actual = transformer.transform(fixture.clone());

        let snapshot = TransformationSnapshot::new("SetModel(gpt-4)_user_only", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }
}
