use derive_setters::Setters;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::{NamedTool, ToolCallFull, ToolDefinition, ToolName};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", content = "value")]
pub enum EventType {
    UserTaskInit(String),
    UserTaskUpdate(String),
    Title(String),

    // For custom events
    Custom { name: String, value: String },
}

impl EventType {
    pub fn name(&self) -> String {
        match self {
            EventType::UserTaskInit(_) => DispatchEvent::USER_TASK_INIT.to_string(),
            EventType::UserTaskUpdate(_) => DispatchEvent::USER_TASK_UPDATE.to_string(),
            EventType::Title(_) => DispatchEvent::TITLE.to_string(),
            EventType::Custom { name, .. } => name.clone(),
        }
    }

    pub fn value(&self) -> String {
        match self {
            EventType::UserTaskInit(value) => value.clone(),
            EventType::UserTaskUpdate(value) => value.clone(),
            EventType::Title(value) => value.clone(),
            EventType::Custom { value, .. } => value.clone(),
        }
    }

    pub fn from_name_value(name: &str, value: &str) -> Self {
        match name {
            DispatchEvent::USER_TASK_INIT => EventType::UserTaskInit(value.to_string()),
            DispatchEvent::USER_TASK_UPDATE => EventType::UserTaskUpdate(value.to_string()),
            DispatchEvent::TITLE => EventType::Title(value.to_string()),
            _ => EventType::Custom { name: name.to_string(), value: value.to_string() },
        }
    }
}

// We'll use simple strings for JSON schema compatibility
#[derive(Debug, JsonSchema, Deserialize, Serialize, Clone)]
pub struct DispatchEvent {
    pub id: String,
    #[serde(flatten)]
    pub event_type: EventType,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Setters)]
pub struct UserContext {
    event: DispatchEvent,
    suggestions: Vec<String>,
}

impl UserContext {
    pub fn new(event: DispatchEvent) -> Self {
        Self { event, suggestions: Default::default() }
    }
}

impl NamedTool for DispatchEvent {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_event_dispatch")
    }
}

impl DispatchEvent {
    pub fn name(&self) -> String {
        self.event_type.name()
    }

    pub fn value(&self) -> String {
        self.event_type.value()
    }

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

    pub fn new(event_type: EventType) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        Self { id, event_type, timestamp }
    }

    pub fn new_name_value(name: impl ToString, value: impl ToString) -> Self {
        Self::new(EventType::from_name_value(
            name.to_string().as_str(),
            value.to_string().as_str(),
        ))
    }

    pub fn task_init(value: impl ToString) -> Self {
        Self::new(EventType::UserTaskInit(value.to_string()))
    }

    pub fn task_update(value: impl ToString) -> Self {
        Self::new(EventType::UserTaskUpdate(value.to_string()))
    }

    pub const USER_TASK_INIT: &'static str = "user_task_init";
    pub const USER_TASK_UPDATE: &'static str = "user_task_update";
    pub const TITLE: &'static str = "title";
}
