use std::collections::HashMap;

use derive_setters::Setters;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::conversation::{EVENT_USER_TASK_INIT, EVENT_USER_TASK_UPDATE};
use crate::{Attachment, NamedTool, ToolDefinition, ToolName};

// We'll use simple strings for JSON schema compatibility
#[derive(Debug, Deserialize, Serialize, Clone, Setters)]
#[setters(into, strip_option)]
pub struct Event {
    pub id: String,
    pub name: String,
    pub value: Option<Value>,
    pub timestamp: String,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, JsonSchema, Deserialize, Serialize, Clone)]
pub struct EventMessage {
    pub name: String,
    pub value: Value,
}

impl From<EventMessage> for Event {
    fn from(value: EventMessage) -> Self {
        Self::new(value.name, Some(value.value))
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Setters)]
pub struct EventContext {
    event: Event,
    suggestions: Vec<String>,
    variables: HashMap<String, Value>,
    current_date: String,
}

impl EventContext {
    pub fn new(event: Event) -> Self {
        Self {
            event,
            suggestions: Default::default(),
            variables: Default::default(),
            current_date: chrono::Local::now().format("%Y-%m-%d").to_string(),
        }
    }

    /// Converts this EventContext into a feedback event by updating the event
    /// name. This should be used when the context already contains user
    /// messages.
    pub fn into_feedback(mut self) -> Self {
        if !self
            .event
            .name
            .ends_with(&format!("/{EVENT_USER_TASK_UPDATE}"))
        {
            self.event.name = format!("{}/{}", self.event.name, EVENT_USER_TASK_UPDATE);
        }
        self
    }

    /// Converts this EventContext into a new task event by updating the event
    /// name. This should be used when this is a new task without prior user
    /// messages.
    pub fn into_task(mut self) -> Self {
        if !self
            .event
            .name
            .ends_with(&format!("/{EVENT_USER_TASK_INIT}"))
        {
            self.event.name = format!("{}/{}", self.event.name, EVENT_USER_TASK_INIT);
        }
        self
    }
}

impl NamedTool for Event {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_event_dispatch")
    }
}

impl Event {
    pub fn tool_definition() -> ToolDefinition {
        ToolDefinition {
            name: Self::tool_name(),
            description: "Dispatches an event with the provided name and value".to_string(),
            input_schema: schema_for!(EventMessage),
        }
    }

    pub fn new<V: Into<Value>>(name: impl ToString, value: Option<V>) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        Self {
            id,
            name: name.to_string(),
            value: value.map(|v| v.into()),
            timestamp,
            attachments: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_into_feedback() {
        let event = Event::new("test_event", None::<String>);
        let context = EventContext::new(event);

        let feedback_context = context.into_feedback();

        assert_eq!(feedback_context.event.name, "test_event/user_task_update");
    }

    #[test]
    fn test_into_task() {
        let event = Event::new("test_event", None::<String>);
        let context = EventContext::new(event);

        let task_context = context.into_task();

        assert_eq!(task_context.event.name, "test_event/user_task_init");
    }

    #[test]
    fn test_into_feedback_prevents_duplicate_suffix() {
        let event = Event::new("test_event", None::<String>);
        let context = EventContext::new(event);

        // Call into_feedback twice
        let feedback_context = context.into_feedback().into_feedback();

        assert_eq!(feedback_context.event.name, "test_event/user_task_update");
    }

    #[test]
    fn test_into_task_prevents_duplicate_suffix() {
        let event = Event::new("test_event", None::<String>);
        let context = EventContext::new(event);

        // Call into_task twice
        let task_context = context.into_task().into_task();

        assert_eq!(task_context.event.name, "test_event/user_task_init");
    }

    #[test]
    fn test_chaining_methods() {
        let event = Event::new("agent_123", Some("initial content"));
        let context = EventContext::new(event).into_task();

        assert_eq!(context.event.name, "agent_123/user_task_init");
        assert_eq!(
            context.event.value,
            Some(serde_json::Value::String("initial content".to_string()))
        );
    }
}
