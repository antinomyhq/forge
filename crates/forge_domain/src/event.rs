use derive_setters::Setters;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::{NamedTool, ToolCallFull, ToolDefinition, ToolName};

// EventType enum to represent different types of events
#[derive(Debug, JsonSchema, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", content = "payload")]
pub enum EventType {
    // Internal pre-defined events
    UserTaskInit(String),
    UserTaskUpdate(String),
    // For custom events
    Custom {
        name: String,
        value: String,
    }
}

impl EventType {
    // Helper to get the name of an event
    pub fn name(&self) -> String {
        match self {
            EventType::UserTaskInit(_) => Event::USER_TASK_INIT.to_string(),
            EventType::UserTaskUpdate(_) => Event::USER_TASK_UPDATE.to_string(),
            EventType::Custom { name, .. } => name.clone(),
        }
    }
    
    // Helper to get the value of an event
    pub fn value(&self) -> String {
        match self {
            EventType::UserTaskInit(value) => value.clone(),
            EventType::UserTaskUpdate(value) => value.clone(),
            EventType::Custom { value, .. } => value.clone(),
        }
    }
}

// Modified Event struct with only EventType
#[derive(Debug, JsonSchema, Deserialize, Serialize, Clone)]
pub struct Event {
    pub id: String,
    pub event_type: EventType,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Setters)]
pub struct EventContext {
    event: Event,
    suggestions: Vec<String>,
}

impl EventContext {
    pub fn new(event: Event) -> Self {
        Self { event, suggestions: Default::default() }
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
            input_schema: schema_for!(Self),
            output_schema: None,
        }
    }

    pub fn parse(tool_call: &ToolCallFull) -> Option<Self> {
        if tool_call.name != Self::tool_definition().name {
            return None;
        }
        serde_json::from_value(tool_call.arguments.clone()).ok()
    }

    // Create a new event with any EventType
    pub fn new(event_type: EventType) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        Self {
            id,
            event_type,
            timestamp,
        }
    }

    // Helper for creating a new custom event (convenience method)
    pub fn new_custom(name: impl ToString, value: impl ToString) -> Self {
        Self::new(EventType::Custom {
            name: name.to_string(),
            value: value.to_string(),
        })
    }

    pub fn task_init(value: impl ToString) -> Self {
        Self::new(EventType::UserTaskInit(value.to_string()))
    }

    pub fn task_update(value: impl ToString) -> Self {
        Self::new(EventType::UserTaskUpdate(value.to_string()))
    }

    // Get the name of an event (convenience method)
    pub fn name(&self) -> String {
        self.event_type.name()
    }

    // Get the value of an event (convenience method)
    pub fn value(&self) -> String {
        self.event_type.value()
    }

    pub const USER_TASK_INIT: &'static str = "user_task_init";
    pub const USER_TASK_UPDATE: &'static str = "user_task_update";
}