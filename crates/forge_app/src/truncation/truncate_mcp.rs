use forge_domain::{ToolOutput, ToolValue};

use super::truncate_text;

/// Result of MCP output truncation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TruncationResult {
    /// Content was within limit, no truncation needed.
    Unchanged(ToolOutput),
    /// Content was truncated, includes truncated output and full JSON for temp file.
    Truncated {
        /// The truncated tool output values.
        truncated_values: Vec<ToolValue>,
        /// The original output serialized as JSON (for writing to temp file).
        full_json: String,
        /// Total size of original text content.
        total_size: usize,
        /// The truncation limit that was applied.
        limit: usize,
        /// Whether the original output was an error.
        is_error: bool,
    },
}

/// Truncates MCP output text values if total text content exceeds the limit.
///
/// Returns `TruncationResult::Unchanged` if no truncation is needed, otherwise
/// returns `TruncationResult::Truncated` with the truncated values and metadata
/// needed to create a temp file with full content.
pub fn truncate_mcp_output(output: ToolOutput, limit: usize) -> anyhow::Result<TruncationResult> {
    // Calculate total text size
    let total_size: usize = output
        .values
        .iter()
        .filter_map(ToolValue::as_str)
        .map(str::len)
        .sum();

    // No truncation needed
    if total_size <= limit {
        return Ok(TruncationResult::Unchanged(output));
    }

    // Serialize full output to JSON for temp file
    let full_json = serde_json::to_string_pretty(&output)?;

    // Truncate text values
    let mut remaining = limit;
    let truncated_values: Vec<ToolValue> = output
        .values
        .into_iter()
        .filter_map(|value| match value {
            ToolValue::Text(text) if remaining > 0 => {
                let truncated = truncate_text(&text, remaining);
                remaining = remaining.saturating_sub(text.len());
                Some(ToolValue::Text(truncated))
            }
            ToolValue::Text(_) => None,
            other => Some(other),
        })
        .collect();

    Ok(TruncationResult::Truncated {
        truncated_values,
        full_json,
        total_size,
        limit,
        is_error: output.is_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_truncate_mcp_output_no_truncation_needed() {
        let fixture = ToolOutput::text("short content");
        let actual = truncate_mcp_output(fixture.clone(), 100).unwrap();
        assert_eq!(actual, TruncationResult::Unchanged(fixture));
    }

    #[test]
    fn test_truncate_mcp_output_exact_boundary() {
        let content = "a".repeat(50);
        let fixture = ToolOutput::text(&content);
        let actual = truncate_mcp_output(fixture.clone(), 50).unwrap();
        assert_eq!(actual, TruncationResult::Unchanged(fixture));
    }

    #[test]
    fn test_truncate_mcp_output_truncates_single_text() {
        let content = "a".repeat(100);
        let fixture = ToolOutput::text(&content);
        let actual = truncate_mcp_output(fixture, 50).unwrap();
        match actual {
            TruncationResult::Truncated {
                truncated_values,
                total_size,
                limit,
                is_error,
                ..
            } => {
                assert_eq!(truncated_values.len(), 1);
                assert_eq!(
                    truncated_values[0],
                    ToolValue::Text("a".repeat(50))
                );
                assert_eq!(total_size, 100);
                assert_eq!(limit, 50);
                assert!(!is_error);
            }
            TruncationResult::Unchanged(_) => panic!("Expected truncation"),
        }
    }

    #[test]
    fn test_truncate_mcp_output_truncates_multiple_texts() {
        let fixture = ToolOutput {
            is_error: false,
            values: vec![
                ToolValue::Text("a".repeat(60)),
                ToolValue::Text("b".repeat(40)),
            ],
        };

        let actual = truncate_mcp_output(fixture, 50).unwrap();
        match actual {
            TruncationResult::Truncated {
                truncated_values,
                total_size,
                limit,
                ..
            } => {
                // First text is truncated to 50, second is dropped (remaining=0)
                assert_eq!(truncated_values.len(), 1);
                assert_eq!(
                    truncated_values[0],
                    ToolValue::Text("a".repeat(50))
                );
                assert_eq!(total_size, 100);
                assert_eq!(limit, 50);
            }
            TruncationResult::Unchanged(_) => panic!("Expected truncation"),
        }
    }

    #[test]
    fn test_truncate_mcp_output_preserves_non_text_values() {
        let fixture = ToolOutput {
            is_error: false,
            values: vec![
                ToolValue::Text("a".repeat(100)),
                ToolValue::Empty,
            ],
        };
        let actual = truncate_mcp_output(fixture, 50).unwrap();
        match actual {
            TruncationResult::Truncated { truncated_values, .. } => {
                assert_eq!(truncated_values.len(), 2);
                assert_eq!(
                    truncated_values[0],
                    ToolValue::Text("a".repeat(50))
                );
                assert_eq!(truncated_values[1], ToolValue::Empty);
            }
            TruncationResult::Unchanged(_) => panic!("Expected truncation"),
        }
    }

    #[test]
    fn test_truncate_mcp_output_preserves_error_flag() {
        let fixture = ToolOutput {
            is_error: true,
            values: vec![ToolValue::Text("a".repeat(100))],
        };
        let actual = truncate_mcp_output(fixture, 50).unwrap();

        match actual {
            TruncationResult::Truncated { is_error, .. } => {
                assert!(is_error);
            }
            TruncationResult::Unchanged(_) => panic!("Expected truncation"),
        }
    }

    #[test]
    fn test_truncate_mcp_output_generates_valid_json() {
        let fixture = ToolOutput::text("a".repeat(100));
        let actual = truncate_mcp_output(fixture.clone(), 50).unwrap();
        match actual {
            TruncationResult::Truncated { full_json, .. } => {
                let parsed: ToolOutput = serde_json::from_str(&full_json).unwrap();
                assert_eq!(parsed, fixture);
            }
            TruncationResult::Unchanged(_) => panic!("Expected truncation"),
        }
    }
}
