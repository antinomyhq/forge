use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use derive_more::derive::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ChatResponse, Context, ContextMessage, Error, Role};

#[derive(Debug, Display, Serialize, Deserialize, Clone, PartialEq, Eq, Copy)]
#[serde(transparent)]
pub struct ConversationId(Uuid);

impl ConversationId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn into_string(&self) -> String {
        self.0.to_string()
    }

    pub fn parse(value: impl ToString) -> Result<Self, Error> {
        Ok(Self(
            Uuid::parse_str(&value.to_string()).map_err(Error::ConversationId)?,
        ))
    }
}

#[derive(Debug, Setters, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ConversationMeta>,
    pub context: Context,
    pub archived: bool,
    pub title: Option<String>,
}

impl Conversation {
    pub fn new(context: Context) -> Self {
        Self {
            id: ConversationId::generate(),
            meta: None,
            context,
            archived: false,
            title: None,
        }
    }

    /// Get the conversation history representation of this conversation
    pub fn history(&self) -> ConversationHistory {
        self.context.clone().into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Represents a conversation's history including user and assistant messages
/// and tool interactions. This is a view-oriented representation of a
/// conversation that is suitable for displaying to users or processing the
/// conversation flow.
#[derive(Debug, Clone, Serialize)]
pub struct ConversationHistory {
    pub messages: Vec<ChatResponse>,
}

impl From<Context> for ConversationHistory {
    fn from(request: Context) -> Self {
        let messages = request
            .messages
            .into_iter()
            .filter(|message| match message {
                ContextMessage::ContentMessage(content) => content.role != Role::System,
                ContextMessage::ToolMessage(_) => true,
            })
            .flat_map(|message| match message {
                ContextMessage::ContentMessage(content) => {
                    let mut messages = vec![ChatResponse::Text(content.content.clone())];
                    if let Some(tool_calls) = content.tool_calls {
                        for tool_call in tool_calls {
                            messages.push(ChatResponse::ToolCallStart(tool_call.clone()));
                        }
                    }
                    messages
                }
                ContextMessage::ToolMessage(result) => {
                    vec![ChatResponse::ToolCallEnd(result)]
                }
            })
            .collect();
        Self { messages }
    }
}

#[async_trait]
pub trait ConversationRepository: Send + Sync {
    /// Set a new conversation or update an existing one
    async fn insert(&self, context: &Context, id: Option<ConversationId>) -> Result<Conversation>;

    /// Get a conversation by its ID
    async fn get(&self, id: ConversationId) -> Result<Conversation>;

    /// List all active (non-archived) conversations
    async fn list(&self) -> Result<Vec<Conversation>>;

    /// Archive a conversation and return the updated conversation
    async fn archive(&self, id: ConversationId) -> Result<Conversation>;

    /// Set the title for a conversation
    async fn set_title(&self, id: &ConversationId, title: String) -> Result<Conversation>;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ContextMessage, Role, ToolCallFull, ToolName, ToolResult};

    #[test]
    fn test_conversation_history_from_context() {
        let mut context = Context::default();

        // Add system message (should be filtered out)
        context = context.add_message(ContextMessage::system("System prompt"));

        // Add user message
        context = context.add_message(ContextMessage::ContentMessage(crate::ContentMessage {
            role: Role::User,
            content: "User message".to_string(),
            tool_calls: None,
        }));

        // Add assistant message with tool call
        let tool_call =
            ToolCallFull::new(ToolName::new("test_tool")).arguments(json!({"arg": "value"}));
        context = context.add_message(ContextMessage::ContentMessage(crate::ContentMessage {
            role: Role::Assistant,
            content: "Assistant message".to_string(),
            tool_calls: Some(vec![tool_call.clone()]),
        }));

        // Add tool result
        let tool_result = ToolResult::new(ToolName::new("test_tool"))
            .success(json!({"result": "success"}).to_string());
        context = context.add_message(ContextMessage::ToolMessage(tool_result.clone()));

        let history = ConversationHistory::from(context);

        assert_eq!(history.messages.len(), 4);
        assert!(matches!(&history.messages[0], ChatResponse::Text(text) if text == "User message"));
        assert!(
            matches!(&history.messages[1], ChatResponse::Text(text) if text == "Assistant message")
        );
        assert!(
            matches!(&history.messages[2], ChatResponse::ToolCallStart(call) if call == &tool_call)
        );
        assert!(
            matches!(&history.messages[3], ChatResponse::ToolCallEnd(result) if result == &tool_result)
        );
    }

    #[test]
    fn test_conversation_history_only_system_message() {
        let context = Context::default().add_message(ContextMessage::system("System prompt"));

        let history = ConversationHistory::from(context);
        assert!(history.messages.is_empty());
    }
}
