use colored::Colorize;
use forge_api::ToolDefinition;
use serde_json::to_string_pretty;

/// Formats the list of tools for display in the shell UI, following these
/// rules:
/// - Name: blue bold
/// - Description: default
/// - Input json schema: dimmed, pretty-printed, multi-line
/// - Blank line between each tool
pub fn format_tools(tools: &[ToolDefinition]) -> String {
    let mut out = String::new();

    for (i, tool) in tools.iter().enumerate() {
        let name = tool.name.as_str().blue().bold();
        let description = &tool.description;
        let schema_json = to_string_pretty(&tool.input_schema).unwrap_or_else(|_| "{}".to_string());
        let schema = format!("{}", schema_json.dimmed());

        if i > 0 {
            out.push('\n');
        }

        out.push_str(&format!("{}\n{}\n{}\n", name, description, schema));
    }

    out
}
