use std::collections::{HashMap, HashSet};

use forge_domain::{Agent, ToolDefinition};

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
    /// based on the agent's configured tool list. Filters and deduplicates
    /// tool definitions based on agent's tools configuration. Returns only
    /// the tool definitions that are specified in the agent's tools list.
    /// Maintains deduplication to avoid duplicate tool definitions.
    pub fn resolve(&self, agent: &Agent) -> Vec<ToolDefinition> {
        // Create a map for efficient tool definition lookup by name
        let tool_definitions_map: HashMap<_, _> = self
            .all_tool_definitions
            .iter()
            .map(|tool| (&tool.name, tool))
            .collect();

        // Deduplicate agent tools before processing
        let unique_agent_tools: HashSet<_> = agent.tools.iter().flatten().collect();

        // Filter and collect tool definitions based on agent's tool list
        unique_agent_tools
            .iter()
            .flat_map(|tool| tool_definitions_map.get(*tool))
            .cloned()
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
}
