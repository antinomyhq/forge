use forge_api::ToolDefinition;

/// Formats the list of tools for display in the shell UI, showing only the tool
/// name as a blue bold heading with numbering for each tool.
pub fn format_tools(tools: &[ToolDefinition]) -> String {
    let mut output = String::new();

    // Calculate the number of digits in the total count
    let max_digits = tools.len().to_string().len();

    for (i, tool) in tools.iter().enumerate() {
        // Add numbered tool name with consistent padding
        output.push_str(&format!(
            "{:>width$}. {}",
            i + 1,
            tool.name,
            width = max_digits
        ));

        // Add newline between tools
        if i < tools.len() - 1 {
            output.push('\n');
        }
    }

    output
}
