use std::collections::HashMap;
use std::sync::OnceLock;

use glob::Pattern;
use merge::Merge;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Error, Result, ToolName};

/// Configuration for filtering MCP tools available to an agent
///
/// Supports three modes:
/// - Boolean: Enable/disable all MCP tools (backward compatible)
/// - Glob: Use glob patterns to match tool names
/// - List: Specify exact tool names to enable
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(untagged)]
pub enum McpToolFilter {
    /// Enable (true) or disable (false) all MCP tools
    Boolean(bool),
    /// Array of glob patterns to match tool names
    Glob(Vec<String>),
    /// Array of exact tool names to enable
    List(Vec<ToolName>),
}

impl Default for McpToolFilter {
    fn default() -> Self {
        Self::Boolean(true)
    }
}
impl From<bool> for McpToolFilter {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl McpToolFilter {
    /// Check if a tool matches this filter
    pub fn matches_tool(&self, tool_name: &ToolName) -> Result<bool> {
        match self {
            Self::Boolean(enabled) => Ok(*enabled),
            Self::List(tools) => Ok(tools.contains(tool_name)),
            Self::Glob(patterns) => {
                if patterns.is_empty() {
                    return Ok(false);
                }

                for pattern_str in patterns {
                    let pattern = Self::compile_glob(pattern_str)?;
                    if pattern.matches(tool_name.as_str()) {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    /// Compile a glob pattern with caching
    fn compile_glob(pattern: &str) -> Result<Pattern> {
        static GLOB_CACHE: OnceLock<std::sync::Mutex<HashMap<String, Pattern>>> = OnceLock::new();

        let cache = GLOB_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
        let mut cache = cache
            .lock()
            .map_err(|e| Error::McpToolFilter(format!("Failed to lock glob cache: {}", e)))?;

        if let Some(pattern) = cache.get(pattern) {
            return Ok(pattern.clone());
        }

        let compiled = Pattern::new(pattern)
            .map_err(|e| Error::McpToolFilter(format!("Invalid pattern '{}': {}", pattern, e)))?;

        cache.insert(pattern.to_string(), compiled.clone());
        Ok(compiled)
    }

    /// Check if this filter allows any tools at all
    pub fn allows_any_tools(&self) -> bool {
        match self {
            Self::Boolean(enabled) => *enabled,
            Self::List(tools) => !tools.is_empty(),
            Self::Glob(patterns) => !patterns.is_empty(),
        }
    }

    /// Filter a list of tool names based on this filter
    pub fn filter_tools(&self, tools: &[ToolName]) -> Result<Vec<ToolName>> {
        let mut filtered = Vec::new();
        for tool in tools {
            if self.matches_tool(tool)? {
                filtered.push(tool.clone());
            }
        }
        Ok(filtered)
    }
}

impl Merge for McpToolFilter {
    fn merge(&mut self, other: Self) {
        // The merging agent's filter takes precedence
        *self = other;
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_boolean_filter_enabled() {
        let fixture = McpToolFilter::Boolean(true);
        let tool_name = ToolName::new("mcp_context7_tool_get_docs");

        let actual = fixture.matches_tool(&tool_name).unwrap();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_boolean_filter_disabled() {
        let fixture = McpToolFilter::Boolean(false);
        let tool_name = ToolName::new("mcp_context7_tool_get_docs");

        let actual = fixture.matches_tool(&tool_name).unwrap();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_glob_filter_wildcard_match() {
        let fixture = McpToolFilter::Glob(vec!["mcp_context7_*".to_string()]);
        let tool_name = ToolName::new("mcp_context7_tool_get_docs");

        let actual = fixture.matches_tool(&tool_name).unwrap();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_glob_filter_no_match() {
        let fixture = McpToolFilter::Glob(vec!["mcp_context7_*".to_string()]);
        let tool_name = ToolName::new("mcp_deepwiki_tool_ask_question");

        let actual = fixture.matches_tool(&tool_name).unwrap();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_glob_filter_multiple_patterns() {
        let fixture = McpToolFilter::Glob(vec![
            "mcp_context7_*".to_string(),
            "mcp_deepwiki_*".to_string(),
        ]);

        let context_tool = ToolName::new("mcp_context7_tool_get_docs");
        let deepwiki_tool = ToolName::new("mcp_deepwiki_tool_ask_question");
        let other_tool = ToolName::new("mcp_other_tool_something");

        assert_eq!(fixture.matches_tool(&context_tool).unwrap(), true);
        assert_eq!(fixture.matches_tool(&deepwiki_tool).unwrap(), true);
        assert_eq!(fixture.matches_tool(&other_tool).unwrap(), false);
    }

    #[test]
    fn test_glob_filter_question_mark_wildcard() {
        let fixture = McpToolFilter::Glob(vec!["mcp_context7_tool_get_???".to_string()]);

        let docs_tool = ToolName::new("mcp_context7_tool_get_doc");
        let library_tool = ToolName::new("mcp_context7_tool_get_library");

        assert_eq!(fixture.matches_tool(&docs_tool).unwrap(), true);
        assert_eq!(fixture.matches_tool(&library_tool).unwrap(), false); // too long
    }

    #[test]
    fn test_glob_filter_empty_patterns() {
        let fixture = McpToolFilter::Glob(vec![]);
        let tool_name = ToolName::new("mcp_context7_tool_get_docs");

        let actual = fixture.matches_tool(&tool_name).unwrap();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_list_filter_exact_match() {
        let fixture = McpToolFilter::List(vec![
            ToolName::new("mcp_context7_tool_get_library_docs"),
            ToolName::new("mcp_deepwiki_tool_ask_question"),
        ]);

        let matched_tool = ToolName::new("mcp_context7_tool_get_library_docs");
        let unmatched_tool = ToolName::new("mcp_context7_tool_get_docs");

        assert_eq!(fixture.matches_tool(&matched_tool).unwrap(), true);
        assert_eq!(fixture.matches_tool(&unmatched_tool).unwrap(), false);
    }

    #[test]
    fn test_list_filter_empty() {
        let fixture = McpToolFilter::List(vec![]);
        let tool_name = ToolName::new("mcp_context7_tool_get_docs");

        let actual = fixture.matches_tool(&tool_name).unwrap();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_allows_any_tools() {
        let enabled_boolean = McpToolFilter::Boolean(true);
        let disabled_boolean = McpToolFilter::Boolean(false);
        let non_empty_glob = McpToolFilter::Glob(vec!["mcp_*".to_string()]);
        let empty_glob = McpToolFilter::Glob(vec![]);
        let non_empty_list = McpToolFilter::List(vec![ToolName::new("tool1")]);
        let empty_list = McpToolFilter::List(vec![]);

        assert_eq!(enabled_boolean.allows_any_tools(), true);
        assert_eq!(disabled_boolean.allows_any_tools(), false);
        assert_eq!(non_empty_glob.allows_any_tools(), true);
        assert_eq!(empty_glob.allows_any_tools(), false);
        assert_eq!(non_empty_list.allows_any_tools(), true);
        assert_eq!(empty_list.allows_any_tools(), false);
    }

    #[test]
    fn test_filter_tools() {
        let fixture = McpToolFilter::Glob(vec!["mcp_context7_*".to_string()]);
        let tools = vec![
            ToolName::new("mcp_context7_tool_get_docs"),
            ToolName::new("mcp_deepwiki_tool_ask_question"),
            ToolName::new("mcp_context7_tool_resolve_library_id"),
            ToolName::new("some_other_tool"),
        ];

        let actual = fixture.filter_tools(&tools).unwrap();
        let expected = vec![
            ToolName::new("mcp_context7_tool_get_docs"),
            ToolName::new("mcp_context7_tool_resolve_library_id"),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_default() {
        let fixture = McpToolFilter::default();
        let expected = McpToolFilter::Boolean(true);
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_merge_strategy() {
        let mut base = McpToolFilter::Boolean(true);
        let other = McpToolFilter::Glob(vec!["mcp_*".to_string()]);

        base.merge(other.clone());
        assert_eq!(base, other);
    }

    #[test]
    fn test_invalid_glob_pattern() {
        let fixture = McpToolFilter::Glob(vec!["[invalid".to_string()]);
        let tool_name = ToolName::new("some_tool");

        let result = fixture.matches_tool(&tool_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_serde_boolean_deserialization() {
        let json = r#"true"#;
        let actual: McpToolFilter = serde_json::from_str(json).unwrap();
        let expected = McpToolFilter::Boolean(true);
        assert_eq!(actual, expected);

        let json = r#"false"#;
        let actual: McpToolFilter = serde_json::from_str(json).unwrap();
        let expected = McpToolFilter::Boolean(false);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serde_glob_deserialization() {
        let json = r#"["mcp_context7_*", "mcp_deepwiki_*"]"#;
        let actual: McpToolFilter = serde_json::from_str(json).unwrap();
        let expected = McpToolFilter::Glob(vec![
            "mcp_context7_*".to_string(),
            "mcp_deepwiki_*".to_string(),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serde_list_deserialization() {
        // With untagged deserialize, string arrays become Glob patterns
        let json = r#"["mcp_context7_tool_get_docs", "mcp_deepwiki_tool_ask_question"]"#;
        let actual: McpToolFilter = serde_json::from_str(json).unwrap();
        let expected = McpToolFilter::Glob(vec![
            "mcp_context7_tool_get_docs".to_string(),
            "mcp_deepwiki_tool_ask_question".to_string(),
        ]);
        assert_eq!(actual, expected);

        // Test that exact match still works with glob patterns that don't contain
        // wildcards
        let tool_name = ToolName::new("mcp_context7_tool_get_docs");
        assert_eq!(actual.matches_tool(&tool_name).unwrap(), true);

        let non_match_tool = ToolName::new("mcp_other_tool");
        assert_eq!(actual.matches_tool(&non_match_tool).unwrap(), false);
    }
}
