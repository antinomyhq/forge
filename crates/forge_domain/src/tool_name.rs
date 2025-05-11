use std::fmt::Display;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
#[serde(transparent)]
pub struct ToolName {
    pub name: String,
    #[serde(skip)]
    pub server: Option<String>,
}

impl ToolName {
    pub fn new(value: impl ToString) -> Self {
        ToolName { name: value.to_string(), server: None }
    }
}

impl Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref server) = self.server {
            write!(f, "{server}")?;
            write!(f, "__")?;
        }

        write!(f, "{}", self.name)
    }
}

pub trait NamedTool {
    fn tool_name() -> ToolName;
}
