use std::collections::HashSet;

use forge_domain::{Agent, ToolDefinition, ToolsDiscriminants};
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
    /// in the agent's tools list. Automatically includes `attempt_completion`
    /// tool as it's required for all agents. Maintains deduplication to avoid
    /// duplicate tool definitions. Returns tools sorted alphabetically by name.
    /// Returns references to avoid unnecessary cloning.
    pub fn resolve<'a>(&'a self, agent: &Agent) -> Vec<&'a ToolDefinition> {
        let matchers = self.build_matchers(agent);
        let mut resolved = self.match_tools(&matchers);
        self.ensure_attempt_completion(&mut resolved);
        self.dedupe_tools(&mut resolved);
        self.sort_tools(&mut resolved);
        resolved
    }

    /// Builds glob matchers from the agent's tool patterns, deduplicating
    /// patterns
    fn build_matchers(&self, agent: &Agent) -> Vec<globset::GlobMatcher> {
        agent
            .tools
            .iter()
            .flatten()
            .collect::<HashSet<_>>()
            .into_iter()
            .filter_map(|pattern| Glob::new(pattern.as_str()).ok())
            .map(|glob| glob.compile_matcher())
            .collect()
    }

    /// Matches tool definitions against glob patterns
    fn match_tools<'a>(&'a self, matchers: &[globset::GlobMatcher]) -> Vec<&'a ToolDefinition> {
        self.all_tool_definitions
            .iter()
            .filter(|tool| {
                matchers
                    .iter()
                    .any(|matcher| matcher.is_match(tool.name.as_str()))
            })
            .collect()
    }

    /// Ensures attempt_completion tool is always included in the resolved tools
    fn ensure_attempt_completion<'a>(&'a self, resolved: &mut Vec<&'a ToolDefinition>) {
        let attempt_completion_name = ToolsDiscriminants::AttemptCompletion.name();

        if resolved
            .iter()
            .any(|tool| tool.name == attempt_completion_name)
        {
            return;
        }

        if let Some(attempt_completion) = self
            .all_tool_definitions
            .iter()
            .find(|tool| tool.name == attempt_completion_name)
        {
            resolved.push(attempt_completion);
        }
    }

    /// Deduplicates tool definitions by name, keeping the first occurrence
    fn dedupe_tools(&self, resolved: &mut Vec<&ToolDefinition>) {
        let mut seen = HashSet::new();
        resolved.retain(|tool| seen.insert(&tool.name));
    }

    /// Sorts tool definitions alphabetically by name
    fn sort_tools(&self, resolved: &mut [&ToolDefinition]) {
        resolved.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
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

        let actual = tool_resolver.resolve(&fixture);
        let expected = vec![
            &tool_resolver.all_tool_definitions[0], // read
            &tool_resolver.all_tool_definitions[2], // search
        ];

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
        let expected: Vec<&ToolDefinition> = vec![];

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
        let expected: Vec<&ToolDefinition> = vec![];

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

        let actual = tool_resolver.resolve(&fixture);
        let expected = vec![
            &tool_resolver.all_tool_definitions[0], // read
            &tool_resolver.all_tool_definitions[1], // write
        ];

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

        let actual = tool_resolver.resolve(&fixture);
        let expected = vec![
            &tool_resolver.all_tool_definitions[0], // fs_read
            &tool_resolver.all_tool_definitions[2], // fs_search
            &tool_resolver.all_tool_definitions[1], // fs_write
        ];

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
        let expected: Vec<&ToolDefinition> = vec![];

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

        let actual = tool_resolver.resolve(&fixture);
        let expected = vec![
            &tool_resolver.all_tool_definitions[0], // fs_read
            &tool_resolver.all_tool_definitions[1], // fs_write
            &tool_resolver.all_tool_definitions[3], // shell
        ];

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

        let actual = tool_resolver.resolve(&fixture);
        let expected = vec![
            &tool_resolver.all_tool_definitions[0], // read1
            &tool_resolver.all_tool_definitions[1], // read2
        ];

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

        let actual = tool_resolver.resolve(&fixture);
        let expected = vec![
            &tool_resolver.all_tool_definitions[0], // fs_read
            &tool_resolver.all_tool_definitions[1], // fs_write
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_always_includes_attempt_completion() {
        let all_tool_definitions = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("write").description("Write Tool"),
            ToolDefinition::new("attempt_completion").description("Completion Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent")).tools(vec![ToolName::new("read")]);

        let actual = tool_resolver.resolve(&fixture);
        let expected = vec![
            &tool_resolver.all_tool_definitions[2], // attempt_completion
            &tool_resolver.all_tool_definitions[0], // read
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_does_not_duplicate_attempt_completion() {
        let all_tool_definitions = vec![
            ToolDefinition::new("read").description("Read Tool"),
            ToolDefinition::new("attempt_completion").description("Completion Tool"),
        ];

        let tool_resolver = ToolResolver::new(all_tool_definitions);

        let fixture = Agent::new(AgentId::new("test-agent")).tools(vec![
            ToolName::new("read"),
            ToolName::new("attempt_completion"),
        ]);

        let actual = tool_resolver.resolve(&fixture);
        let expected = vec![
            &tool_resolver.all_tool_definitions[1], // attempt_completion
            &tool_resolver.all_tool_definitions[0], // read
        ];

        assert_eq!(actual, expected);
    }
}
