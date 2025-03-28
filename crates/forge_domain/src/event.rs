use std::collections::HashMap;

use derive_setters::Setters;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{NamedTool, ToolCallFull, ToolDefinition, ToolName};

// We'll use simple strings for JSON schema compatibility
#[derive(Debug, Deserialize, Serialize, Clone, Setters)]
pub struct Event {
    pub id: String,
    pub name: String,
    pub value: Value,
    pub timestamp: String,
}

#[derive(Debug, JsonSchema, Deserialize, Serialize, Clone)]
pub struct EventMessage {
    pub name: String,
    pub value: Value,
}

impl From<EventMessage> for Event {
    fn from(value: EventMessage) -> Self {
        Self::new(value.name, value.value)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Setters)]
pub struct EventContext {
    event: Event,
    suggestions: Vec<String>,
    variables: HashMap<String, Value>,
}

impl EventContext {
    pub fn new(event: Event) -> Self {
        Self {
            event,
            suggestions: Default::default(),
            variables: Default::default(),
        }
    }
}

impl NamedTool for Event {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_event_dispatch")
    }
}

impl Event {
    pub fn tool_definition() -> ToolDefinition {
        ToolDefinition {
            name: Self::tool_name(),
            description: "Dispatches an event with the provided name and value".to_string(),
            input_schema: schema_for!(EventMessage),
            output_schema: None,
        }
    }

    pub fn parse(tool_call: &ToolCallFull) -> Option<Self> {
        if tool_call.name != Self::tool_definition().name {
            return None;
        }
        let message: Option<EventMessage> =
            serde_json::from_value(tool_call.arguments.clone()).ok();

        message.map(|message| message.into())
    }

    pub fn new<V: Into<Value>>(name: impl ToString, value: V) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        Self { id, name: name.to_string(), value: value.into(), timestamp }
    }
}
