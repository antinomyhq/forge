use chrono::{DateTime, Utc};
use derive_more::derive::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Context, Error, Metrics, Result};

// Event type constants
pub const EVENT_USER_TASK_INIT: &str = "user_task_init";
pub const EVENT_USER_TASK_UPDATE: &str = "user_task_update";

#[derive(Debug, Default, Display, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ConversationId(Uuid);

impl Copy for ConversationId {}

impl ConversationId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn into_string(&self) -> String {
        self.0.to_string()
    }

    pub fn parse(value: impl ToString) -> Result<Self> {
        Ok(Self(
            Uuid::parse_str(&value.to_string()).map_err(Error::ConversationId)?,
        ))
    }
}

#[derive(Debug, Setters, Serialize, Deserialize, Clone)]
#[setters(into)]
pub struct Conversation {
    pub id: ConversationId,

    pub title: Option<String>,
    pub context: Option<Context>,
    pub metrics: Metrics,
    pub metadata: MetaData,
}

#[derive(Debug, Setters, Serialize, Deserialize, Clone)]
#[setters(into)]
pub struct MetaData {
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl MetaData {
    pub fn new(created_at: DateTime<Utc>) -> Self {
        Self { created_at, updated_at: None }
    }
}

impl Conversation {
    pub fn new(id: ConversationId) -> Self {
        let created_at = Utc::now();
        let metrics = Metrics::new().with_time(created_at);
        Self {
            id,
            metrics,
            metadata: MetaData::new(created_at),
            title: None,
            context: None,
        }
    }
    /// Creates a new conversation with a new conversation ID.
    ///
    /// This is a convenience constructor that automatically generates a unique
    /// conversation ID, making it easy to create new conversations without
    /// having to manually create the ID.
    pub fn generate() -> Self {
        Self::new(ConversationId::generate())
    }

    /// Generates an HTML representation of the conversation
    ///
    /// This method uses Handlebars to render the conversation as HTML
    /// from the template file, including all agents, events, and variables.
    ///
    /// # Errors
    /// - If the template file cannot be found or read
    /// - If the Handlebars template registration fails
    /// - If the template rendering fails
    pub fn to_html(&self) -> String {
        // Instead of using Handlebars, we now use our Element DSL
        crate::conversation_html::render_conversation_html(self)
    }

    /// Gets a summary of the conversation's current state.
    ///
    /// This method provides quick access to:
    /// - The last attempt completion (if any)
    /// - The user message that preceded it
    /// - Any detected interruptions
    ///
    /// # Returns
    /// - `Some(ConversationSummary)` if the conversation has context
    /// - `None` if the conversation has no context yet
    pub fn get_summary(&self) -> Option<crate::ConversationSummary> {
        self.context.as_ref().map(|ctx| ctx.get_summary())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{Context, ContextMessage, ToolCallFull, ToolCallId, ToolName};

    #[test]
    fn test_get_summary_without_context() {
        let fixture = Conversation::generate();

        let actual = fixture.get_summary();

        assert!(actual.is_none());
    }

    #[test]
    fn test_get_summary_with_context_and_completion() {
        let context = Context::default()
            .add_message(ContextMessage::user("Create a test file", None))
            .add_message(ContextMessage::assistant(
                "File created",
                None,
                Some(vec![ToolCallFull {
                    name: ToolName::new("attempt_completion"),
                    call_id: Some(ToolCallId::new("call1")),
                    arguments: crate::ToolCallArguments::from(serde_json::json!({
                        "result": "# Task Complete\n\nFile created successfully"
                    })),
                }]),
            ));

        let fixture = Conversation::generate().context(context);

        let actual = fixture.get_summary();

        assert!(actual.is_some());
        let summary = actual.unwrap();
        assert_eq!(summary.user_message, Some("Create a test file".to_string()));
        assert!(summary.completion.is_some());
        assert_eq!(
            summary.completion.unwrap().result,
            "# Task Complete\n\nFile created successfully"
        );
        assert!(summary.interruption.is_none());
    }

    #[test]
    fn test_get_summary_with_interrupted_conversation() {
        let context = Context::default()
            .add_message(ContextMessage::user("Long running task", None))
            .add_message(ContextMessage::assistant("Processing...", None, None));

        let fixture = Conversation::generate().context(context);

        let actual = fixture.get_summary();

        assert!(actual.is_some());
        let summary = actual.unwrap();
        assert!(summary.completion.is_none());
        assert!(summary.interruption.is_some());
    }
}
