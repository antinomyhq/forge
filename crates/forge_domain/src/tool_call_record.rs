use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{ToolCallFull, ToolResult};

/// Represents a complete tool invocation cycle, containing both the original
/// call and its corresponding result.
#[derive(Clone, Debug, Deserialize, Serialize, Setters)]
#[setters(strip_option, into)]
pub struct ToolCallRecord {
    pub tool_call: ToolCallFull,
    pub tool_result: ToolResult,
}

impl ToolCallRecord {
    /// Creates a new CallRecord from a tool call and its result
    pub fn new(call: ToolCallFull, result: ToolResult) -> Self {
        Self { tool_call: call, tool_result: result }
    }
}
