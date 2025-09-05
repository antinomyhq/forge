use convert_case::{Case, Casing};
use forge_api::ToolsOverview;

use crate::info::Info;

/// Formats the tools overview for display using the Info component,
/// organized by categories with availability checkboxes.
pub fn format_tools(overview: &ToolsOverview) -> Info {
    let mut info = Info::new();
    let checkbox = "[✓]";

    // System tools section
    info = info.add_title("SYSTEM");
    let available_system_tools: std::collections::HashSet<&str> = overview
        .system
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();

    for tool_name in &available_system_tools {
        info = info.add_key(format!("{} {}", checkbox, tool_name));
    }

    // Agents section
    info = info.add_title("AGENTS");
    let available_agent_tools: std::collections::HashSet<&str> = overview
        .agents
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();

    for agent_name in &available_agent_tools {
        info = info.add_key(format!("{} {}", checkbox, agent_name));
    }

    // MCP tools section
    if !overview.mcp.is_empty() {
        for (server_name, tools) in &overview.mcp {
            let title = server_name.to_case(Case::UpperSnake);
            info = info.add_title(title);

            for tool in tools {
                // MCP tools are always available if they're in the list
                info = info.add_key(format!("  [✓] {}", tool.name.as_str()));
            }
        }
    }

    info
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_api::ToolDefinition;

    use super::*;

    fn create_tool_definition(name: &str) -> ToolDefinition {
        ToolDefinition::new(name)
    }

    fn create_tools_overview() -> ToolsOverview {
        ToolsOverview::new()
    }

    #[test]
    fn test_format_tools_with_system_tools_only() {
        let fixture = create_tools_overview().system(vec![
            create_tool_definition("read"),
            create_tool_definition("write"),
            create_tool_definition("search"),
        ]);

        let actual = format_tools(&fixture);
        let expected = format!("{}", actual);

        // Verify it contains the System section
        assert!(expected.contains("System"));
        assert!(expected.contains("[✓] read"));
        assert!(expected.contains("[✓] write"));
        assert!(expected.contains("[✓] search"));
        assert!(expected.contains("[ ] patch"));
        assert!(expected.contains("[ ] undo"));
        assert!(expected.contains("[ ] shell"));
        assert!(expected.contains("[ ] fetch"));
        assert!(expected.contains("[ ] remove"));
    }

    #[test]
    fn test_format_tools_with_agents() {
        let fixture = create_tools_overview().agents(vec![
            create_tool_definition("forge"),
            create_tool_definition("sage"),
        ]);

        let actual = format_tools(&fixture);
        let expected = format!("{}", actual);

        // Verify it contains the Agents section
        assert!(expected.contains("Agents"));
        assert!(expected.contains("[✓] forge"));
        assert!(expected.contains("[✓] sage"));
        assert!(expected.contains("[ ] muse"));
        assert!(expected.contains("[ ] prime"));
        assert!(expected.contains("[ ] parker"));
    }

    #[test]
    fn test_format_tools_with_mcp_tools() {
        let mut mcp_map = HashMap::new();
        mcp_map.insert(
            "code".to_string(),
            vec![
                create_tool_definition("code_tool_1"),
                create_tool_definition("code_tool_2"),
            ],
        );
        mcp_map.insert(
            "mcp".to_string(),
            vec![create_tool_definition("custom_tool")],
        );

        let fixture = create_tools_overview()
            .system(vec![create_tool_definition("read")])
            .mcp(mcp_map);

        let actual = format_tools(&fixture);
        let expected = format!("{}", actual);

        // Verify it contains MCP sections
        assert!(expected.contains("MCP: code"));
        assert!(expected.contains("[✓] code_tool_1"));
        assert!(expected.contains("[✓] code_tool_2"));
        assert!(expected.contains("MCP"));
        assert!(expected.contains("[✓] custom_tool"));
    }

    #[test]
    fn test_format_tools_complete_example() {
        let mut mcp_map = HashMap::new();
        mcp_map.insert(
            "code".to_string(),
            vec![create_tool_definition("code_tool_1")],
        );
        mcp_map.insert("db".to_string(), vec![create_tool_definition("db_query")]);

        let fixture = create_tools_overview()
            .system(vec![
                create_tool_definition("read"),
                create_tool_definition("write"),
                create_tool_definition("shell"),
            ])
            .agents(vec![
                create_tool_definition("forge"),
                create_tool_definition("sage"),
            ])
            .mcp(mcp_map);

        let actual = format_tools(&fixture);
        let expected = format!("{}", actual);

        // Verify all sections are present
        assert!(expected.contains("System"));
        assert!(expected.contains("Agents"));
        assert!(expected.contains("MCP: code"));
        assert!(expected.contains("MCP: db"));

        // Verify checkboxes are correct
        assert!(expected.contains("[✓] read"));
        assert!(expected.contains("[ ] search")); // not available
        assert!(expected.contains("[✓] forge"));
        assert!(expected.contains("[ ] muse")); // not available
        assert!(expected.contains("[✓] code_tool_1"));
        assert!(expected.contains("[✓] db_query"));
    }
}
