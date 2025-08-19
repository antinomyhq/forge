use std::collections::HashMap;

use derive_setters::Setters;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    current_time: String,
}

impl EventContext {
    pub fn new(event: Event) -> Self {
        Self {
            event,
            suggestions: Default::default(),
            variables: Default::default(),
            current_time: chrono::Local::now()
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string(),
        }
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
