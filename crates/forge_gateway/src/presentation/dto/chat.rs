use forge_domain::{ContentMessage, Context, ContextMessage, Role};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct ChatCompletionRequest {
    #[validate(length(min = 1, message = "model cannot be empty"))]
    pub model: String,
    #[validate(length(min = 1, message = "messages cannot be empty"))]
    pub messages: Vec<Message>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl From<ChatCompletionRequest> for Context {
    fn from(req: ChatCompletionRequest) -> Self {
        let context = Context::default();

        // Convert messages to ContextMessages
        let messages = req
            .messages
            .into_iter()
            .map(|m| {
                let role = match m.role.as_str() {
                    "system" => Role::System,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                ContextMessage::ContentMessage(ContentMessage {
                    role,
                    content: m.content,
                    tool_calls: None,
                })
            })
            .collect();

        context.extend_messages(messages)
    }
}
