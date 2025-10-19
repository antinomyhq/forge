use forge_domain::TitleFormat;

use crate::title_display::TitleDisplayExt;

/// Display a single configuration field (kept simple for scriptability)
pub fn display_single_field(field: &str, value: Option<String>) {
    match value {
        Some(v) => println!("{v}"),
        None => eprintln!("{field}: Not set"),
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
}
