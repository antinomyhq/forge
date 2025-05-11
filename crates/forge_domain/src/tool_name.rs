use derive_setters::Setters;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct ToolName {
    pub name: String,
    pub server: Option<String>,
}

impl ToolName {
    pub fn new(value: impl ToString) -> Self {
        ToolName { name: value.to_string(), server: None }
    }
}

impl ToolName {
    pub fn to_string(&self) -> String {
        match self.server {
            None => self.name.clone(),
            Some(ref server) => format!("{}__{}", server, self.name),
        }
    }
}

pub trait NamedTool {
    fn tool_name() -> ToolName;
}
