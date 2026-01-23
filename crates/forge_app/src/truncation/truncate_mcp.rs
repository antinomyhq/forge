use derive_setters::Setters;
use forge_template::Element;

/// Output structure for MCP truncation with metadata
#[derive(Debug, Clone, PartialEq, Eq, Default, Setters)]
#[setters(strip_option, into)]
pub struct TruncatedMcpOutput {
    pub content: String,
    pub total_lines: usize,
    pub display_start_line: usize,
    pub display_end_line: usize,
    pub is_truncated: bool,
}

impl TruncatedMcpOutput {
    /// Converts the truncated output into an XML representation
    ///
    /// # Arguments
    /// * `temp_file_path` - Path to the file containing the full content
    ///
    /// # Returns
    /// XML string with truncation metadata and content
    pub fn to_xml(&self, temp_file_path: &str) -> String {
        let mut element = Element::new("mcp_output")
            .attr("start_line", self.display_start_line)
            .attr("end_line", self.display_end_line)
            .attr("total_lines", self.total_lines)
            .append(Element::new("body").cdata(&self.content));

        if self.is_truncated {
            element = element.attr("file_path", temp_file_path);
            let reason = format!(
                "Content truncated to first {} of {} lines. Full content available at: {}",
                self.display_end_line, self.total_lines, temp_file_path
            );
            element = element.append(Element::new("truncated").text(reason));
        }

        element.render()
    }
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
/// `TruncatedMcpOutput` containing the truncated content and metadata
pub fn truncate_mcp_content(
    content: &str,
    prefix_lines: usize,
    max_line_length: usize,
) -> TruncatedMcpOutput {
    // Handle empty content
    if content.is_empty() {
        return TruncatedMcpOutput {
            content: String::new(),
            total_lines: 0,
            display_start_line: 1,
            display_end_line: 0,
            is_truncated: false,
        };
    }

    let total_lines = content.lines().count();

    // Reuse shell truncation logic with suffix=0 (prefix only)
    let (result_lines, truncation_info, _truncated_lines_count) =
        super::clip_by_lines(content, prefix_lines, 0, max_line_length);

    let is_truncated = truncation_info.is_some();
    let display_end_line = if is_truncated {
        prefix_lines
    } else {
        total_lines
    };

    TruncatedMcpOutput {
        content: result_lines.join("\n"),
        total_lines,
        display_start_line: 1,
        display_end_line,
        is_truncated,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_to_xml() {
        let truncated = TruncatedMcpOutput {
            content: "Line 1\nLine 2".to_string(),
            total_lines: 5,
            display_start_line: 1,
            display_end_line: 2,
            is_truncated: true,
        };
        let actual = truncated.to_xml("/tmp/test.txt");
        assert!(actual.contains("start_line=\"1\""));
        assert!(actual.contains("end_line=\"2\""));
        assert!(actual.contains("total_lines=\"5\""));
        assert!(actual.contains("file_path=\"/tmp/test.txt\""));
        assert!(actual.contains("Line 1\nLine 2"));
        assert!(actual.contains("Content truncated to first 2 of 5 lines"));
        assert!(actual.contains("<truncated>"));
    }

    #[test]
    fn test_to_xml_not_truncated() {
        let not_truncated = TruncatedMcpOutput {
            content: "Line 1\nLine 2\nLine 3".to_string(),
            total_lines: 3,
            display_start_line: 1,
            display_end_line: 3,
            is_truncated: false,
        };
        let actual = not_truncated.to_xml("/tmp/test.txt");
        assert!(actual.contains("start_line=\"1\""));
        assert!(actual.contains("end_line=\"3\""));
        assert!(actual.contains("total_lines=\"3\""));
        assert!(actual.contains("Line 1\nLine 2\nLine 3"));
        // Should NOT contain file_path, truncated element, or truncation message
        assert!(!actual.contains("file_path"));
        assert!(!actual.contains("<truncated>"));
        assert!(!actual.contains("Content truncated"));
    }

    #[test]
    fn test_empty_content() {
        let fixture = "";
        let actual = truncate_mcp_content(fixture, 10, 2000);
        let expected = TruncatedMcpOutput {
            content: String::new(),
            total_lines: 0,
            display_start_line: 1,
            display_end_line: 0,
            is_truncated: false,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_truncation_prefix_only() {
        let fixture = [
            "line 1", "line 2", "line 3", "line 4", "line 5", "line 6", "line 7",
        ]
        .join("\n");
        let actual = truncate_mcp_content(&fixture, 3, 2000);
        let expected = TruncatedMcpOutput {
            content: "line 1\nline 2\nline 3".to_string(),
            total_lines: 7,
            display_start_line: 1,
            display_end_line: 3,
            is_truncated: true,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_truncation_when_content_fits() {
        let fixture = ["line 1", "line 2", "line 3"].join("\n");
        let actual = truncate_mcp_content(&fixture, 10, 2000);
        let expected = TruncatedMcpOutput {
            content: "line 1\nline 2\nline 3".to_string(),
            total_lines: 3,
            display_start_line: 1,
            display_end_line: 3,
            is_truncated: false,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_prefix_only() {
        let fixture = ["line 1", "line 2", "line 3", "line 4", "line 5"].join("\n");
        let actual = truncate_mcp_content(&fixture, 2, 2000);
        let expected = TruncatedMcpOutput {
            content: "line 1\nline 2".to_string(),
            total_lines: 5,
            display_start_line: 1,
            display_end_line: 2,
            is_truncated: true,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_exact_match() {
        let fixture = ["line 1", "line 2", "line 3"].join("\n");
        let actual = truncate_mcp_content(&fixture, 3, 2000);
        let expected = TruncatedMcpOutput {
            content: "line 1\nline 2\nline 3".to_string(),
            total_lines: 3,
            display_start_line: 1,
            display_end_line: 3,
            is_truncated: false,
        };
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
        let expected = TruncatedMcpOutput {
            content: "short line\nthis is a very ...[41 more chars truncated]\nanother short l...[3 more chars truncated]"
                .to_string(),
            total_lines: 3,
            display_start_line: 1,
            display_end_line: 3,
            is_truncated: false,
        };
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
        let expected = TruncatedMcpOutput {
            content: "line 1\nvery long ...[46 more chars truncated]".to_string(),
            total_lines: 7,
            display_start_line: 1,
            display_end_line: 2,
            is_truncated: true,
        };
        assert_eq!(actual, expected);
    }
}
