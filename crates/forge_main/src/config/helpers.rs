use crate::cli_format::format_columns;

/// Format and display configuration values
///
/// This helper function provides a centralized way to format and display
/// configuration values, avoiding code duplication between different command
/// handlers.
pub fn format_config_list(agent: Option<String>, model: Option<String>, provider: Option<String>) {
    let agent_val = agent.unwrap_or_else(|| "Not set".to_string());
    let model_val = model.unwrap_or_else(|| "Not set".to_string());
    let provider_val = provider.unwrap_or_else(|| "Not set".to_string());

    let configs = vec![
        ("Agent".to_string(), agent_val),
        ("Model".to_string(), model_val),
        ("Provider".to_string(), provider_val),
    ];

    format_columns(configs);
}
