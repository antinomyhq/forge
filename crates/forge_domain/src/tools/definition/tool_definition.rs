use std::cmp::Ordering;

use derive_setters::Setters;
use schemars::schema::RootSchema;
use serde::{Deserialize, Serialize};

use crate::ToolName;

///
/// Refer to the specification over here:
/// https://glama.ai/blog/2024-11-25-model-context-protocol-quickstart#server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct ToolDefinition {
    pub name: ToolName,
    pub description: String,
    pub input_schema: RootSchema,
}

impl Ord for ToolDefinition {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl Eq for ToolDefinition {}

impl PartialOrd for ToolDefinition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl ToolDefinition {
    /// Create a new ToolDefinition
    pub fn new<N: ToString>(name: N) -> Self {
        ToolDefinition {
            name: ToolName::new(name),
            description: String::new(),
            input_schema: schemars::schema_for!(()), // Empty input schema
        }
    }
}

pub trait ToolDescription {
    fn description(&self) -> String;
}
