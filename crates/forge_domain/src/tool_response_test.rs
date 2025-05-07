#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use crate::{ToolCallId, ToolName, ToolResponseData, ToolResult};

    #[test]
    fn test_file_read_front_matter() {
        let data = ToolResponseData::FileRead {
            path: "/path/to/file.txt".to_string(),
            total_chars: Some(1000),
            char_range: Some((0, 500)),
            is_binary: Some(false),
        };

        let content = "This is the file content";
        let front_matter = data.to_front_matter(content);

        println!("Front matter content: {}", front_matter);

        // Check that the front matter contains the expected fields
        assert!(front_matter.contains("---"));
        assert!(front_matter.contains("type: file_read"));
        assert!(front_matter.contains("path: /path/to/file.txt"));
        assert!(front_matter.contains("total_chars: 1000"));
        assert!(front_matter.contains("char_range: 0-500"));
        assert!(front_matter.contains("is_binary: false"));
        assert!(front_matter.contains("This is the file content"));
    }

    #[test]
    fn test_shell_front_matter() {
        let data = ToolResponseData::Shell {
            command: "ls -la".to_string(),
            exit_code: Some(0),
            total_chars: Some(500),
            truncated: Some(false),
        };

        let content = "drwxr-xr-x 1 user group 0 Jan 1 00:00 .";
        let front_matter = data.to_front_matter(content);

        println!("Shell front matter content: {}", front_matter);

        // Check that the front matter contains the expected fields
        assert!(front_matter.contains("---"));
        assert!(front_matter.contains("type: shell"));
        assert!(front_matter.contains("command: 'ls -la'"));
        assert!(front_matter.contains("exit_code: 0"));
        assert!(front_matter.contains("total_chars: 500"));
        assert!(front_matter.contains("truncated: false"));
        assert!(front_matter.contains("drwxr-xr-x 1 user group 0 Jan 1 00:00 ."));
    }

    #[test]
    fn test_tool_result_display() {
        let result = ToolResult::new(ToolName::new("fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .with_data(ToolResponseData::FileRead {
                path: "/path/to/file.txt".to_string(),
                total_chars: Some(1000),
                char_range: Some((0, 500)),
                is_binary: Some(false),
            })
            .success("This is the file content");

        let display = result.to_string();
        println!("Display content: {}", display);

        // Check that the display contains the expected fields
        assert!(display.contains("---"));
        assert!(display.contains("type: file_read"));
        assert!(display.contains("path: /path/to/file.txt"));
        assert!(display.contains("tool_name: fs_read"));
        assert!(display.contains("status: Success"));
        assert!(display.contains("call_id: call_123"));
        assert!(display.contains("This is the file content"));
    }

    #[test]
    fn test_generic_tool_response() {
        let mut metadata = HashMap::new();
        metadata.insert("custom_field".to_string(), json!("custom_value"));
        metadata.insert("number_field".to_string(), json!(42));

        let data = ToolResponseData::generic(metadata);
        let content = "Generic content";
        let front_matter = data.to_front_matter(content);

        println!("Generic front matter content: {}", front_matter);

        // Check that the front matter contains the expected fields
        assert!(front_matter.contains("---"));
        assert!(front_matter.contains("type: generic"));
        assert!(front_matter.contains("custom_field: custom_value"));
        assert!(front_matter.contains("number_field: 42"));
        assert!(front_matter.contains("Generic content"));
    }
}
