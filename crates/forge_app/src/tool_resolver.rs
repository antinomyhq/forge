use std::collections::HashSet;

use forge_domain::{Agent, ToolDefinition};
use globset::Glob;

/// Service that resolves tool definitions for agents based on their configured
/// tool list
pub struct ToolResolver {
    all_tool_definitions: Vec<ToolDefinition>,
}

impl ToolResolver {
    /// Creates a new ToolResolver with all available tool definitions
    pub fn new(all_tool_definitions: Vec<ToolDefinition>) -> Self {
        Self { all_tool_definitions }
    }

    /// Resolves the tool definitions for a specific agent by filtering
    /// based on the agent's configured tool list. Supports both exact matches
    /// and glob patterns (e.g., "fs_*" matches "fs_read", "fs_write").
    /// Filters and deduplicates tool definitions based on agent's tools
    /// configuration. Returns only the tool definitions that are specified
    /// in the agent's tools list. Maintains deduplication to avoid
    /// duplicate tool definitions.
    pub fn resolve(&self, agent: &Agent) -> Vec<ToolDefinition> {
        // Build glob matchers from unique agent tools
        let matchers: Vec<_> = agent
            .tools
            .iter()
            .flatten()
            .collect::<HashSet<_>>()
            .into_iter()
            .filter_map(|pattern| Glob::new(pattern.as_str()).ok())
            .map(|glob| glob.compile_matcher())
            .collect();

        // Match tools against all patterns and deduplicate
        self.all_tool_definitions
            .iter()
            .filter(|tool| {
                matchers
                    .iter()
                    .any(|matcher| matcher.is_match(tool.name.as_str()))
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Agent, AgentId, ToolDefinition, ToolName};
    use pretty_assertions::assert_eq;

    use super::ToolResolver;

    #[test]
    fn test_resolve_filters_agent_tools() {
        let all_tool_definitions = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("write").description("Write Tool"),
            ToolDefinition::new("search").description("Search Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent"))
            .tools(vec![ToolName::new("read"), ToolName::new("search")]);

        let mut actual = tool_resolver.resolve(&fixture);
        let mut expected = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("search").description("Search Tool"),
        ];

        // Sort both vectors by tool name for deterministic comparison
        actual.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_with_no_agent_tools() {
        let all_tool_definitions = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("write").description("Write Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent"));

        let actual = tool_resolver.resolve(&fixture);
        let expected: Vec<ToolDefinition> = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_with_nonexistent_tools() {
        let all_tool_definitions = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("write").description("Write Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent")).tools(vec![
            ToolName::new("nonexistent1"),
            ToolName::new("nonexistent2"),
        ]);

        let actual = tool_resolver.resolve(&fixture);
        let expected: Vec<ToolDefinition> = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_with_duplicate_agent_tools() {
        let all_tool_definitions = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("write").description("Write Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent")).tools(vec![
            ToolName::new("read"),
            ToolName::new("read"), // Duplicate
            ToolName::new("write"),
        ]);

        let mut actual = tool_resolver.resolve(&fixture);
        let mut expected = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("write").description("Write Tool"),
        ];

        // Sort both vectors by tool name for deterministic comparison
        actual.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_with_glob_pattern_wildcard() {
        let all_tool_definitions = vec![
            ToolDefinition::new("fs_read").description("Read Tool"),
            ToolDefinition::new("fs_write").description("Write Tool"),
            ToolDefinition::new("fs_search").description("Search Tool"),
            ToolDefinition::new("net_fetch").description("Fetch Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent")).tools(vec![ToolName::new("fs_*")]);

        let mut actual = tool_resolver.resolve(&fixture);
        let mut expected = vec![
            ToolDefinition::new("fs_read").description("Read Tool"),
            ToolDefinition::new("fs_write").description("Write Tool"),
            ToolDefinition::new("fs_search").description("Search Tool"),
        ];

        // Sort both vectors by tool name for deterministic comparison
        actual.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_with_glob_pattern_no_matches() {
        let all_tool_definitions = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("write").description("Write Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent")).tools(vec![ToolName::new("fs_*")]);

        let actual = tool_resolver.resolve(&fixture);
        let expected: Vec<ToolDefinition> = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_with_mixed_exact_and_glob() {
        let all_tool_definitions = vec![
            ToolDefinition::new("fs_read").description("FS Read Tool"),
            ToolDefinition::new("fs_write").description("FS Write Tool"),
            ToolDefinition::new("net_fetch").description("Net Fetch Tool"),
            ToolDefinition::new("shell").description("Shell Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent"))
            .tools(vec![ToolName::new("fs_*"), ToolName::new("shell")]);

        let mut actual = tool_resolver.resolve(&fixture);
        let mut expected = vec![
            ToolDefinition::new("fs_read").description("FS Read Tool"),
            ToolDefinition::new("fs_write").description("FS Write Tool"),
            ToolDefinition::new("shell").description("Shell Tool"),
        ];

        // Sort both vectors by tool name for deterministic comparison
        actual.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_with_question_mark_wildcard() {
        let all_tool_definitions = vec![
            ToolDefinition::new("read1").description("Read 1 Tool"),
            ToolDefinition::new("read2").description("Read 2 Tool"),
            ToolDefinition::new("read10").description("Read 10 Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent")).tools(vec![ToolName::new("read?")]);

        let mut actual = tool_resolver.resolve(&fixture);
        let mut expected = vec![
            ToolDefinition::new("read1").description("Read 1 Tool"),
            ToolDefinition::new("read2").description("Read 2 Tool"),
        ];

        // Sort both vectors by tool name for deterministic comparison
        actual.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_with_overlapping_glob_patterns() {
        let all_tool_definitions = vec![
            ToolDefinition::new("fs_read").description("FS Read Tool"),
            ToolDefinition::new("fs_write").description("FS Write Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent")).tools(vec![
            ToolName::new("fs_*"),
            ToolName::new("fs_read"),
            ToolName::new("*_read"),
        ]);

        let mut actual = tool_resolver.resolve(&fixture);
        let mut expected = vec![
            ToolDefinition::new("fs_read").description("FS Read Tool"),
            ToolDefinition::new("fs_write").description("FS Write Tool"),
        ];

        // Sort both vectors by tool name for deterministic comparison
        actual.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        assert_eq!(actual, expected);
    }
}
