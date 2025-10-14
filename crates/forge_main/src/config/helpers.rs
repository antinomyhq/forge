use crate::info::Info;

/// Build configuration info struct
///
/// This helper function provides a centralized way to build configuration
/// information as an Info struct, enabling consistent formatting across
/// normal and porcelain output modes.
pub fn build_config_info(
    agent: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Info {
    let agent_val = agent.unwrap_or_else(|| "Not set".to_string());
    let model_val = model.unwrap_or_else(|| "Not set".to_string());
    let provider_val = provider.unwrap_or_else(|| "Not set".to_string());

    Info::new()
        .add_title("CONFIGURATION")
        .add_key_value("Agent", agent_val)
        .add_key_value("Model", model_val)
        .add_key_value("Provider", provider_val)
}
