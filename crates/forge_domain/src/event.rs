use std::collections::HashMap;

use derive_more::{Deref, From};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::conversation::{EVENT_USER_TASK_INIT, EVENT_USER_TASK_UPDATE};
use crate::{Attachment, NamedTool, Template, ToolName};

/// Represents a partial event structure used for CLI event dispatching
///
/// This is an intermediate structure for parsing event JSON from the CLI
/// before converting it to a full Event type.
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UserCommand {
    pub name: String,
    pub template: Template<Value>,
    pub parameters: Vec<String>,
}

impl UserCommand {
    pub fn new<V: Into<Template<Value>>>(
        name: impl ToString,
        value: V,
        parameters: Vec<String>,
    ) -> Self {
        Self { name: name.to_string(), template: value.into(), parameters }
    }
}

impl From<UserCommand> for Event {
    fn from(value: UserCommand) -> Self {
        Event::new(value.name.clone(), Some(EventValue::Command(value)))
    }
}

impl<T: AsRef<str>> From<T> for EventValue {
    fn from(value: T) -> Self {
        EventValue::Text(UserPrompt(value.as_ref().to_owned()))
    }
}

// We'll use simple strings for JSON schema compatibility
#[derive(Debug, Deserialize, Serialize, Clone, Setters)]
#[setters(into, strip_option)]
pub struct Event {
    pub id: String,
    pub name: String,
    pub value: Option<EventValue>,
    pub timestamp: String,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum EventValue {
    Text(UserPrompt),
    Command(UserCommand),
}

impl EventValue {
    pub fn as_user_prompt(&self) -> Option<&UserPrompt> {
        match self {
            EventValue::Text(user_prompt) => Some(user_prompt),
            EventValue::Command(_) => None,
        }
    }

    pub fn as_command(&self) -> Option<&UserCommand> {
        match self {
            EventValue::Text(_user_prompt) => None,
            EventValue::Command(user_command) => Some(user_command),
        }
    }

    pub fn text(str: impl ToString) -> Self {
        EventValue::Text(UserPrompt(str.to_string()))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, From, Deref)]
#[serde(transparent)]
pub struct UserPrompt(String);

#[derive(Clone, Serialize, Deserialize, Debug, Setters)]
pub struct EventContext {
    event: EventContextValue,
    suggestions: Vec<String>,
    variables: HashMap<String, Value>,
    current_date: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Setters)]
pub struct EventContextValue {
    pub name: String,
    pub value: String,
}

impl EventContextValue {
    pub fn new<S: Into<String>>(name: S, value: S) -> Self {
        Self { name: name.into(), value: value.into() }
    }
}

impl EventContext {
    pub fn new(event: impl Into<EventContextValue>) -> Self {
        Self {
            event: event.into(),
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
    pub fn new<V: Into<EventValue>>(name: impl ToString, value: Option<V>) -> Self {
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
        let event = EventContextValue::new("test_event", "");
        let context = EventContext::new(event);

        let feedback_context = context.into_feedback();

        assert_eq!(feedback_context.event.name, "test_event/user_task_update");
    }

    #[test]
    fn test_into_task() {
        let event = EventContextValue::new("test_event", "");
        let context = EventContext::new(event);

        let task_context = context.into_task();

        assert_eq!(task_context.event.name, "test_event/user_task_init");
    }

    #[test]
    fn test_into_feedback_prevents_duplicate_suffix() {
        let event = EventContextValue::new("test_event", "");
        let context = EventContext::new(event);

        // Call into_feedback twice
        let feedback_context = context.into_feedback().into_feedback();

        assert_eq!(feedback_context.event.name, "test_event/user_task_update");
    }

    #[test]
    fn test_into_task_prevents_duplicate_suffix() {
        let event = EventContextValue::new("test_event", "");
        let context = EventContext::new(event);

        // Call into_task twice
        let task_context = context.into_task().into_task();

        assert_eq!(task_context.event.name, "test_event/user_task_init");
    }

    #[test]
    fn test_chaining_methods() {
        let event = EventContextValue::new("agent_123", "initial content");
        let context = EventContext::new(event).into_task();

        assert_eq!(context.event.name, "agent_123/user_task_init");
        assert_eq!(context.event.value, "initial content");
    }
}
