use derive_setters::Setters;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::{NamedTool, ToolCallFull, ToolDefinition, ToolName};

// We'll use simple strings for JSON schema compatibility
#[derive(Debug, JsonSchema, Deserialize, Serialize, Clone)]
pub struct DispatchEvent {
    pub id: String,
    pub name: String,
    pub value: String,
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
    pub fn tool_definition() -> ToolDefinition {
        ToolDefinition {
            name: Self::tool_name(),
            description: "Dispatches a custom event with the provided name and value".to_string(),
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

    /// Creates a new dispatch event with the given name and value
    /// Returns an error if the name is invalid
    pub fn new(name: impl ToString, value: impl ToString) -> anyhow::Result<Self> {
        let name = name.to_string();
        if !Self::validate_name(&name) {
            return Err(anyhow::anyhow!("Invalid event name: must be non-empty and contain only alphanumeric characters, underscores, or dashes"));
        }

        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            value: value.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Validates if the event name follows the required format
    pub fn validate_name(name: &str) -> bool {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    }

    pub fn task_init(value: impl ToString) -> Self {
        // These are internal constant names, so we can safely unwrap
        Self::new(Self::USER_TASK_INIT, value).unwrap()
    }

    pub fn task_update(value: impl ToString) -> Self {
        // These are internal constant names, so we can safely unwrap
        Self::new(Self::USER_TASK_UPDATE, value).unwrap()
    }

    pub const USER_TASK_INIT: &'static str = "user_task_init";
    pub const USER_TASK_UPDATE: &'static str = "user_task_update";
}
