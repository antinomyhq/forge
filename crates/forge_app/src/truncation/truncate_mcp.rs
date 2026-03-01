use std::path::Path;

use forge_template::Element;

/// MCP output that was actually truncated (compile-time guarantee)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatedMcpOutput {
    pub content: String,
    pub total_lines: usize,
    pub display_start_line: usize,
    pub display_end_line: usize,
}

impl TruncatedMcpOutput {
    /// Converts truncated output into XML representation with temp file
    /// reference
    ///
    /// # Arguments
    /// * `temp_file_path` - Path to the file containing the full content
    pub fn into_xml(self, temp_file_path: impl AsRef<Path>) -> String {
        let temp_file_path = temp_file_path.as_ref().display().to_string();
        let reason = format!(
            "Content truncated to first {} of {} lines. Full content available at: {}",
            self.display_end_line, self.total_lines, temp_file_path
        );

        Element::new("mcp_output")
            .attr("start_line", self.display_start_line)
            .attr("end_line", self.display_end_line)
            .attr("total_lines", self.total_lines)
            .attr("file_path", temp_file_path)
            .append(Element::new("body").cdata(&self.content))
            .append(Element::new("truncated").text(reason))
            .render()
    }
}

/// MCP output that fits within the limit (no truncation needed)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteMcpOutput {
    pub content: String,
}

/// Result of MCP truncation - either truncated or complete
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTruncationResult {
    Truncated(TruncatedMcpOutput),
    Complete(CompleteMcpOutput),
}

/// Truncates MCP content based on line limit with prefix-only strategy
///
/// Reuses the shell truncation logic via `clip_by_lines` but only keeps the
/// first N lines. MCP output only cares about the prefix.
///
/// # Arguments
/// * `content` - The text content to truncate
/// * `prefix_lines` - Number of lines to keep from the beginning
/// * `max_line_length` - Maximum length for individual lines (longer lines get
///   truncated)
///
/// # Returns
/// `McpTruncationResult` - either `Truncated` or `Complete`
pub fn truncate_mcp_content(
    content: &str,
    prefix_lines: usize,
    max_line_length: usize,
) -> McpTruncationResult {
    let total_lines = content.lines().count();
    // Reuse shell truncation logic with suffix=0 (prefix only)
    let (result_lines, truncation_info, _truncated_lines_count) =
        super::clip_by_lines(content, prefix_lines, 0, max_line_length);
    let content = result_lines.join("\n");
    if truncation_info.is_some() {
        McpTruncationResult::Truncated(TruncatedMcpOutput {
            content,
            total_lines,
            display_start_line: 1,
            display_end_line: prefix_lines,
        })
    } else {
        McpTruncationResult::Complete(CompleteMcpOutput { content })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_into_xml() {
        let truncated = TruncatedMcpOutput {
            content: "Line 1\nLine 2".to_string(),
            total_lines: 5,
            display_start_line: 1,
            display_end_line: 2,
        };
        let actual = truncated.into_xml("/tmp/test.txt");
        let expected = r#"<mcp_output
  start_line="1"
  end_line="2"
  total_lines="5"
  file_path="/tmp/test.txt"
>
<body><![CDATA[Line 1
Line 2]]></body>
<truncated>Content truncated to first 2 of 5 lines. Full content available at: /tmp/test.txt</truncated>
</mcp_output>"#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_content() {
        let fixture = "";
        let actual = truncate_mcp_content(fixture, 10, 2000);
        let expected = McpTruncationResult::Complete(CompleteMcpOutput { content: String::new() });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_truncation_prefix_only() {
        let fixture = [
            "line 1", "line 2", "line 3", "line 4", "line 5", "line 6", "line 7",
        ]
        .join("\n");
        let actual = truncate_mcp_content(&fixture, 3, 2000);
        let expected = McpTruncationResult::Truncated(TruncatedMcpOutput {
            content: "line 1\nline 2\nline 3".to_string(),
            total_lines: 7,
            display_start_line: 1,
            display_end_line: 3,
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_truncation_when_content_fits() {
        let fixture = ["line 1", "line 2"].join("\n");
        let actual = truncate_mcp_content(&fixture, 10, 2000);
        let expected = McpTruncationResult::Complete(CompleteMcpOutput {
            content: "line 1\nline 2".to_string(),
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_prefix_only() {
        let fixture = ["line 1", "line 2", "line 3", "line 4", "line 5"].join("\n");
        let actual = truncate_mcp_content(&fixture, 2, 2000);
        let expected = McpTruncationResult::Truncated(TruncatedMcpOutput {
            content: "line 1\nline 2".to_string(),
            total_lines: 5,
            display_start_line: 1,
            display_end_line: 2,
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_exact_match() {
        let fixture = ["line 1", "line 2", "line 3"].join("\n");
        let actual = truncate_mcp_content(&fixture, 3, 2000);
        let expected = McpTruncationResult::Complete(CompleteMcpOutput {
            content: "line 1\nline 2\nline 3".to_string(),
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_long_line_truncation() {
        let fixture = [
            "short line",
            "this is a very long line that exceeds the maximum length",
            "another short line",
        ]
        .join("\n");
        let actual = truncate_mcp_content(&fixture, 10, 15);
        let expected = McpTruncationResult::Complete(CompleteMcpOutput {
            content: "short line\nthis is a very ...[41 more chars truncated]\nanother short l...[3 more chars truncated]"
                .to_string(),
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_long_lines_with_line_count_truncation() {
        let fixture = [
            "line 1",
            "very long line that will be truncated at character level",
            "line 3",
            "line 4",
            "line 5",
            "line 6",
            "line 7",
        ]
        .join("\n");
        let actual = truncate_mcp_content(&fixture, 2, 10);
        let expected = McpTruncationResult::Truncated(TruncatedMcpOutput {
            content: "line 1\nvery long ...[46 more chars truncated]".to_string(),
            total_lines: 7,
            display_start_line: 1,
            display_end_line: 2,
        });
        assert_eq!(actual, expected);
    }
}
