use std::cmp::Ordering;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ToolDefinition, ToolName};

/// Defines the ordering for tools in an agent's context.
/// Tools are ordered based on the list provided, with glob patterns supported.
/// When the list is empty, tools are sorted alphabetically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[derive(Default)]
pub struct ToolOrder(Vec<ToolName>);


impl ToolOrder {
    /// Creates a new ToolOrder with the specified tool names
    ///
    /// # Arguments
    ///
    /// * `tools` - List of tool names (and patterns) to use as the basis for
    ///   ordering
    pub fn new(tools: Vec<ToolName>) -> Self {
        Self(tools)
    }

    /// Creates a ToolOrder from a list of tool names, using the exact order
    /// as specified in the list, including glob patterns.
    ///
    /// # Arguments
    ///
    /// * `tools` - List of tool names (and patterns) to use as the basis for
    ///   ordering
    pub fn from_tool_list(tools: &[ToolName]) -> Self {
        if tools.is_empty() {
            return Self::default();
        }

        Self(tools.to_vec())
    }

    /// Returns true if this is an empty order (alphabetical sorting)
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the tool names in this order
    pub fn tools(&self) -> &[ToolName] {
        &self.0
    }

    /// Sorts tool definitions according to the ordering strategy
    ///
    /// # Arguments
    ///
    /// * `tools` - Mutable slice of tool definitions to sort
    pub fn sort(&self, tools: &mut [ToolDefinition]) {
        if self.0.is_empty() {
            // Empty order means alphabetical
            tools.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        } else {
            tools.sort_by(|a, b| self.compare_with_custom_order(&a.name, &b.name));
        }
    }

    /// Sorts tool definition references according to the ordering strategy
    ///
    /// # Arguments
    ///
    /// * `tools` - Mutable slice of tool definition references to sort
    pub fn sort_refs(&self, tools: &mut [&ToolDefinition]) {
        if self.0.is_empty() {
            // Empty order means alphabetical
            tools.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        } else {
            tools.sort_by(|a, b| self.compare_with_custom_order(&a.name, &b.name));
        }
    }

    /// Compares two tool names based on custom ordering
    ///
    /// Tools in the custom order list come first, in the order specified.
    /// This handles both exact matches and glob pattern matches.
    /// Tools not in the list come after, sorted alphabetically.
    fn compare_with_custom_order(&self, a: &ToolName, b: &ToolName) -> Ordering {
        use glob::Pattern;

        // Helper to find position considering both exact match and glob patterns
        let find_position = |tool: &ToolName| -> Option<usize> {
            // First try exact match
            if let Some(pos) = self.0.iter().position(|name| name == tool) {
                return Some(pos);
            }

            // Then try glob pattern match
            for (pos, pattern_name) in self.0.iter().enumerate() {
                if let Ok(pattern) = Pattern::new(pattern_name.as_str())
                    && pattern.matches(tool.as_str()) {
                        return Some(pos);
                    }
            }

            None
        };

        let a_pos = find_position(a);
        let b_pos = find_position(b);

        match (a_pos, b_pos) {
            // Both tools are in the custom order list (or match patterns)
            (Some(a_idx), Some(b_idx)) => {
                if a_idx == b_idx {
                    // Both match the same pattern, sort alphabetically
                    a.as_str().cmp(b.as_str())
                } else {
                    a_idx.cmp(&b_idx)
                }
            }
            // Only 'a' is in the custom order list, so it comes first
            (Some(_), None) => Ordering::Less,
            // Only 'b' is in the custom order list, so it comes first
            (None, Some(_)) => Ordering::Greater,
            // Neither tool is in the custom order list, sort alphabetically
            (None, None) => a.as_str().cmp(b.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_alphabetical_sort() {
        let fixture_order = ToolOrder::new(vec![]); // Empty list = alphabetical
        let mut fixture = vec![
            ToolDefinition::new("zebra").description("Z tool"),
            ToolDefinition::new("alpha").description("A tool"),
            ToolDefinition::new("beta").description("B tool"),
        ];

        fixture_order.sort(&mut fixture);

        let actual: Vec<String> = fixture.iter().map(|t| t.name.to_string()).collect();
        let expected = vec!["alpha", "beta", "zebra"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_custom_order_all_specified() {
        let fixture_order = ToolOrder::new(vec![
            ToolName::new("beta"),
            ToolName::new("alpha"),
            ToolName::new("zebra"),
        ]);
        let mut fixture = vec![
            ToolDefinition::new("zebra").description("Z tool"),
            ToolDefinition::new("alpha").description("A tool"),
            ToolDefinition::new("beta").description("B tool"),
        ];

        fixture_order.sort(&mut fixture);

        let actual: Vec<String> = fixture.iter().map(|t| t.name.to_string()).collect();
        let expected = vec!["beta", "alpha", "zebra"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_custom_order_partial_specification() {
        let fixture_order = ToolOrder::new(vec![ToolName::new("zebra"), ToolName::new("beta")]);
        let mut fixture = vec![
            ToolDefinition::new("alpha").description("A tool"),
            ToolDefinition::new("beta").description("B tool"),
            ToolDefinition::new("zebra").description("Z tool"),
            ToolDefinition::new("delta").description("D tool"),
            ToolDefinition::new("charlie").description("C tool"),
        ];

        fixture_order.sort(&mut fixture);

        let actual: Vec<String> = fixture.iter().map(|t| t.name.to_string()).collect();
        // zebra and beta come first (in that order), rest alphabetically
        let expected = vec!["zebra", "beta", "alpha", "charlie", "delta"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_custom_order_with_refs() {
        let fixture_order = ToolOrder::new(vec![ToolName::new("write"), ToolName::new("read")]);
        let tools = vec![
            ToolDefinition::new("read").description("Read tool"),
            ToolDefinition::new("write").description("Write tool"),
            ToolDefinition::new("patch").description("Patch tool"),
        ];
        let mut fixture: Vec<&ToolDefinition> = tools.iter().collect();

        fixture_order.sort_refs(&mut fixture);

        let actual: Vec<String> = fixture.iter().map(|t| t.name.to_string()).collect();
        let expected = vec!["write", "read", "patch"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_custom_order_empty_list() {
        let fixture_order = ToolOrder::new(vec![]);
        let mut fixture = vec![
            ToolDefinition::new("zebra").description("Z tool"),
            ToolDefinition::new("alpha").description("A tool"),
            ToolDefinition::new("beta").description("B tool"),
        ];

        fixture_order.sort(&mut fixture);

        let actual: Vec<String> = fixture.iter().map(|t| t.name.to_string()).collect();
        // Should fall back to alphabetical
        let expected = vec!["alpha", "beta", "zebra"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_tool_list_exact_order() {
        let fixture = vec![
            ToolName::new("write"),
            ToolName::new("read"),
            ToolName::new("sage"),
            ToolName::new("patch"),
            ToolName::new("sem_search"),
        ];

        let actual = ToolOrder::from_tool_list(&fixture);

        let names: Vec<String> = actual.tools().iter().map(|t| t.to_string()).collect();
        // Should maintain exact order as specified
        assert_eq!(names[0], "write");
        assert_eq!(names[1], "read");
        assert_eq!(names[2], "sage");
        assert_eq!(names[3], "patch");
        assert_eq!(names[4], "sem_search");
    }

    #[test]
    fn test_from_tool_list_with_mcp_tools() {
        let fixture = vec![
            ToolName::new("read"),
            ToolName::new("mcp_github"),
            ToolName::new("write"),
            ToolName::new("mcp_slack"),
            ToolName::new("patch"),
        ];

        let actual = ToolOrder::from_tool_list(&fixture);

        let names: Vec<String> = actual.tools().iter().map(|t| t.to_string()).collect();
        // Should maintain exact order as specified, no special rules
        assert_eq!(names[0], "read");
        assert_eq!(names[1], "mcp_github");
        assert_eq!(names[2], "write");
        assert_eq!(names[3], "mcp_slack");
        assert_eq!(names[4], "patch");
    }

    #[test]
    fn test_from_tool_list_empty() {
        let fixture: Vec<ToolName> = vec![];

        let actual = ToolOrder::from_tool_list(&fixture);

        assert_eq!(actual, ToolOrder::new(vec![]));
    }

    #[test]
    fn test_from_tool_list_with_glob_patterns() {
        let fixture = vec![
            ToolName::new("read"),
            ToolName::new("fs_*"), // Glob pattern - preserved
            ToolName::new("write"),
            ToolName::new("mcp_*"), // Glob pattern - preserved
            ToolName::new("patch"),
        ];

        let actual = ToolOrder::from_tool_list(&fixture);

        let names: Vec<String> = actual.tools().iter().map(|t| t.to_string()).collect();
        // All tools and patterns preserved
        assert_eq!(names.len(), 5);
        assert_eq!(names[0], "read");
        assert_eq!(names[1], "fs_*");
        assert_eq!(names[2], "write");
        assert_eq!(names[3], "mcp_*");
        assert_eq!(names[4], "patch");
    }

    #[test]
    fn test_custom_order_with_glob_pattern_matching() {
        let fixture_order = ToolOrder::new(vec![
            ToolName::new("read"),
            ToolName::new("fs_*"),
            ToolName::new("shell"),
        ]);
        let mut fixture = vec![
            ToolDefinition::new("shell").description("Shell tool"),
            ToolDefinition::new("fs_write").description("FS Write"),
            ToolDefinition::new("read").description("Read tool"),
            ToolDefinition::new("fs_read").description("FS Read"),
        ];

        fixture_order.sort(&mut fixture);

        let actual: Vec<String> = fixture.iter().map(|t| t.name.to_string()).collect();
        // read (pos 0), fs_read and fs_write (both match fs_* at pos 1, alphabetically
        // sorted), shell (pos 2)
        let expected = vec!["read", "fs_read", "fs_write", "shell"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_empty() {
        let empty = ToolOrder::new(vec![]);
        let non_empty = ToolOrder::new(vec![ToolName::new("read")]);

        assert!(empty.is_empty());
        assert!(!non_empty.is_empty());
    }
}
