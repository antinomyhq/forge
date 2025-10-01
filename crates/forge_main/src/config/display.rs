use colored::Colorize;

/// Display configuration value with proper formatting
pub fn display_config_value(key: &str, value: Option<String>) -> String {
    match value {
        Some(v) => format!("{}: {}", key.bold(), v.green()),
        None => format!("{}: {}", key.bold(), "Not set".yellow()),
    }
}

/// Display all configuration values in a formatted table
pub fn display_all_config(agent: Option<String>, model: Option<String>, provider: Option<String>) {
    println!("\n{}", "Current Configuration:".bold().underline());
    println!("  {}", display_config_value("Agent", agent));
    println!("  {}", display_config_value("Model", model));
    println!("  {}", display_config_value("Provider", provider));
    println!();
}

/// Display a single configuration field
pub fn display_single_field(field: &str, value: Option<String>) {
    match value {
        Some(v) => println!("{}", v),
        None => eprintln!("{}: Not set", field),
    }
}

/// Display success message for configuration update
pub fn display_success(field: &str, value: &str) {
    println!("{} {} to {}", "✓".green(), field, value.bold());
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_display_config_value_with_value() {
        let fixture = display_config_value("Agent", Some("forge".to_string()));
        let actual = fixture.contains("Agent") && fixture.contains("forge");
        assert_eq!(actual, true);
    }

    #[test]
    fn test_display_config_value_without_value() {
        let fixture = display_config_value("Model", None);
        let actual = fixture.contains("Model") && fixture.contains("Not set");
        assert_eq!(actual, true);
    }
}
