use crate::open_router::request::{OpenRouterRequest, OpenRouterRole};
use crate::open_router::transformers::Transformer;

/// Transformer that caches the last user/system message for supported models
pub struct SetCache;

impl Transformer for SetCache {
    fn transform(&self, mut request: OpenRouterRequest) -> OpenRouterRequest {
        if let (Some(mut messages), Some(model)) = (request.messages.take(), request.model.take()) {
            if let Some(msg) = messages
                .iter_mut()
                .rev()
                .find(|msg| matches!(msg.role, OpenRouterRole::System | OpenRouterRole::User))
            {
                msg.content = msg.content.take().map(|content| content.cached());
            }

            request.messages = Some(messages);
            request.model = Some(model);
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{ContentMessage, Context, ContextMessage, ModelId, Role};

    use super::*;
    use crate::open_router::request::MessageContent;

    #[test]
    fn test_sonnet_transformer_caching() {
        let context = Context {
            messages: vec![ContextMessage::ContentMessage(ContentMessage {
                role: Role::User,
                content: "test message".to_string(),
                tool_calls: None,
            })],
            tools: vec![],
            tool_choice: None,
        };

        let request =
            OpenRouterRequest::from(context).model(ModelId::new("anthropic/claude-3.5-sonnet"));

        let transformer = SetCache;
        let transformed = transformer.transform(request);

        let messages = transformed.messages.unwrap();
        assert!(matches!(
            messages[0].content,
            Some(MessageContent::Parts(_))
        ));
    }
}
