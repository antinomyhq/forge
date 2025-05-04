use std::fmt::Display;

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

/// Formats the CallRecord as XML with tool name, arguments, and result
impl Display for ToolCallRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tool_name = self.tool_call.name.as_str();

        writeln!(f, "---")?;
        writeln!(f, "tool_name: {tool_name}")?;
        if self.tool_result.is_error {
            writeln!(f, "status: Failure")?;
        }
        writeln!(f, "---")?;

        writeln!(f, "{}", self.tool_result.content)?;

        Ok(())
    }
}

impl ToolCallRecord {
    /// Creates a new CallRecord from a tool call and its result
    pub fn new(call: ToolCallFull, result: ToolResult) -> Self {
        Self { tool_call: call, tool_result: result }
    }

    /// Returns true if the tool execution was successful
    pub fn is_success(&self) -> bool {
        !self.tool_result.is_error
    }

    /// Returns true if the tool execution resulted in an error
    pub fn is_error(&self) -> bool {
        self.tool_result.is_error
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use serde_json::json;

    use super::*;
    use crate::{ToolCallId, ToolName};

    #[test]
    fn test_call_record_creation() {
        // Create a tool call
        let call = ToolCallFull {
            name: ToolName::new("test_tool"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"arg1": "value1", "arg2": 42}),
        };

        // Create a successful result
        let result = ToolResult::new(ToolName::new("test_tool"))
            .call_id(ToolCallId::new("call_123"))
            .success("Operation completed successfully");

        // Create a CallRecord
        let record = ToolCallRecord::new(call, result);

        // Verify it's successful
        assert!(record.is_success());
        assert!(!record.is_error());
    }

    #[test]
    fn test_call_record_with_error() {
        // Create a tool call
        let call = ToolCallFull {
            name: ToolName::new("test_tool"),
            call_id: Some(ToolCallId::new("call_456")),
            arguments: json!({"path": "/nonexistent/path"}),
        };

        // Create an error result
        let result = ToolResult::new(ToolName::new("test_tool"))
            .call_id(ToolCallId::new("call_456"))
            .failure(anyhow::anyhow!("File not found"));

        // Create a CallRecord
        let record = ToolCallRecord::new(call, result);

        // Verify it's an error
        assert!(record.is_error());
        assert!(!record.is_success());
    }

    #[test]
    fn test_display_successful_call() {
        // Create a tool call with simple arguments
        let call = ToolCallFull {
            name: ToolName::new("fs_read"),
            call_id: Some(ToolCallId::new("call_abc123")),
            arguments: json!({"path": "/example/file.txt"}),
        };

        // Create a successful result
        let result = ToolResult::new(ToolName::new("fs_read"))
            .call_id(ToolCallId::new("call_abc123"))
            .success("Contents of the file");

        // Create a CallRecord
        let record = ToolCallRecord::new(call, result);

        // Check the formatted output
        assert_snapshot!(record.to_string());
    }

    #[test]
    fn test_display_failed_call() {
        // Create a tool call
        let call = ToolCallFull {
            name: ToolName::new("fs_write"),
            call_id: Some(ToolCallId::new("call_def456")),
            arguments: json!({
                "path": "/path/to/file.txt",
                "content": "Example content",
                "overwrite": false
            }),
        };

        // Create an error result
        let result = ToolResult::new(ToolName::new("fs_write"))
            .call_id(ToolCallId::new("call_def456"))
            .failure(anyhow::anyhow!("Permission denied"));

        // Create a CallRecord
        let record = ToolCallRecord::new(call, result);

        // Check the formatted output
        assert_snapshot!(record.to_string());
    }

    #[test]
    fn test_display_with_special_chars() {
        // Create a tool call with arguments containing special XML characters
        let call = ToolCallFull {
            name: ToolName::new("test_tool"),
            call_id: None,
            arguments: json!({
                "text": "Special chars: < > & ' \"",
                "html": "<div>Test</div>"
            }),
        };

        // Create a result with special characters
        let result = ToolResult::new(ToolName::new("test_tool"))
            .success("Result with <tags> & special \"characters\"");

        // Create a CallRecord
        let record = ToolCallRecord::new(call, result);

        // Check the formatted output properly escapes special characters
        assert_snapshot!(record.to_string());
    }
}
