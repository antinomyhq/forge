use std::collections::HashSet;

use convert_case::{Case, Casing};
use forge_api::{ToolName, ToolsOverview};

use crate::info::Info;

/// Formats the tools overview for display using the Info component,
/// organized by categories with availability checkboxes.
pub fn format_tools(agent_tools: &[ToolName], overview: &ToolsOverview) -> Info {
    let mut info = Info::new();
    let agent_tools = agent_tools.iter().collect::<HashSet<_>>();
    let checkbox = |tool_name: &ToolName| -> &str {
        if agent_tools.contains(tool_name) {
            "[✓]"
        } else {
            "[ ]"
        }
    };

    // System tools section
    info = info.add_title("SYSTEM");
    for tool in &overview.system {
        info = info.add_value(format!("{} {}", checkbox(&tool.name), tool.name));
    }

    // Agents section
    info = info.add_title("AGENTS");
    for tool in &overview.agents {
        info = info.add_value(format!("{} {}", checkbox(&tool.name), tool.name));
    }

    // MCP tools section
    if !overview.mcp.get_servers().is_empty() {
        for (server_name, tools) in overview.mcp.get_servers().iter() {
            let title = (*server_name).to_case(Case::UpperSnake);
            info = info.add_title(title);

            for tool in tools {
                info = info.add_value(format!("{} {}", checkbox(&tool.name), tool.name));
            }
        }
    }

    // Failed MCP servers section
    if !overview.mcp.get_failures().is_empty() {
        info = info.add_title("FAILED MCP SERVERS");
        for (server_name, error) in overview.mcp.get_failures().iter() {
            // Truncate error message for readability in list view
            // Use 'mcp show <name>' for full error details
            let truncated_error = if error.len() > 80 {
                format!("{}...", &error[..77])
            } else {
                error.clone()
            };
            info = info.add_value(format!("[✗] {server_name} - {truncated_error}"));
        }
    }

    info
}
