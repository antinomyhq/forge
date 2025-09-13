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
        if agent_tools.contains(&tool_name) {
            "[✓]"
        } else {
            "[ ]"
        }
    };

    // System tools section
    info = info.add_title("SYSTEM");
    for tool in &overview.system {
        info = info.add_key(format!("{} {}", checkbox(&tool.name), tool.name));
    }

    // Agents section
    info = info.add_title("AGENTS");
    for tool in &overview.agents {
        info = info.add_key(format!("{} {}", checkbox(&tool.name), tool.name));
    }

    // MCP tools section
    if !overview.mcp.is_empty() {
        for (server_name, tools) in &overview.mcp {
            let title = server_name.to_case(Case::UpperSnake);
            info = info.add_title(title);

            for tool in tools {
                // MCP tools are always available if they're in the list
                info = info.add_key(format!("[✓] {}", tool.name.as_str()));
            }
        }
    }

    info
}
