use forge_domain::TitleFormat;

use crate::title_display::TitleDisplayExt;

/// Display all configuration values in a standardized TitleFormat style
pub fn display_all_config(agent: Option<String>, model: Option<String>, provider: Option<String>) {
    let agent_val = agent.unwrap_or_else(|| "Not set".to_string());
    let model_val = model.unwrap_or_else(|| "Not set".to_string());
    let provider_val = provider.unwrap_or_else(|| "Not set".to_string());

    println!(
        "{}",
        TitleFormat::info("Agent").sub_title(agent_val).display()
    );
    println!(
        "{}",
        TitleFormat::info("Model").sub_title(model_val).display()
    );
    println!(
        "{}",
        TitleFormat::info("Provider")
            .sub_title(provider_val)
            .display()
    );
}

/// Display a single configuration field (kept simple for scriptability)
pub fn display_single_field(field: &str, value: Option<String>) {
    match value {
        Some(v) => println!("{}", v),
        None => eprintln!("{}: Not set", field),
    }
}

/// Display success message for configuration update using TitleFormat
pub fn display_success(field: &str, value: &str) {
    println!("{}", TitleFormat::action(field).sub_title(value).display());
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::title_display::TitleDisplayExt;

    #[test]
    fn test_display_success_titleformat_plain() {
        // Compose a deterministic string by disabling timestamp and colors
        let fixture = TitleFormat::action("Agent").sub_title("forge");
        let actual = fixture
            .display_with_timestamp(false)
            .with_colors(false)
            .to_string();
        let expected_contains = ["⏺ ", "Agent", " forge"]; // plain formatter adds a space before subtitle
        assert_eq!(expected_contains.iter().all(|p| actual.contains(p)), true);
    }

    #[test]
    fn test_display_all_config_title_lines_plain() {
        // Verify that info titles can be composed deterministically without
        // colors/timestamps
        let agent = TitleFormat::info("Agent").sub_title("forge");
        let model = TitleFormat::info("Model").sub_title("gpt-4o");
        let provider = TitleFormat::info("Provider").sub_title("openai");

        let outputs = vec![agent, model, provider]
            .into_iter()
            .map(|t| {
                t.display_with_timestamp(false)
                    .with_colors(false)
                    .to_string()
            })
            .collect::<Vec<_>>();

        assert_eq!(outputs[0].contains("Agent"), true);
        assert_eq!(outputs[0].contains(" forge"), true);
        assert_eq!(outputs[1].contains("Model"), true);
        assert_eq!(outputs[1].contains(" gpt-4o"), true);
        assert_eq!(outputs[2].contains("Provider"), true);
        assert_eq!(outputs[2].contains(" openai"), true);
    }
}
