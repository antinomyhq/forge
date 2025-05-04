use derive_setters::Setters;
use regex::Regex;
use termimad::crossterm::style::{Attribute, Color};
use termimad::{CompoundStyle, LineStyle, MadSkin};

/// MarkdownFormat provides functionality for formatting markdown text for
/// terminal display.
#[derive(Clone, Setters, Default)]
#[setters(into, strip_option)]
pub struct MarkdownFormat {
    skin: MadSkin,
    max_consecutive_newlines: usize,
}

impl MarkdownFormat {
    /// Create a new MarkdownFormat with the default skin
    pub fn new() -> Self {
        let mut skin = MadSkin::default();
        let compound_style = CompoundStyle::new(Some(Color::Cyan), None, Attribute::Bold.into());
        skin.inline_code = compound_style.clone();

        let mut codeblock_style = CompoundStyle::new(None, None, Default::default());
        codeblock_style.add_attr(Attribute::Dim);

        skin.code_block = LineStyle::new(codeblock_style, Default::default());

        Self { skin, max_consecutive_newlines: 2 }
    }

    /// Render the markdown content to a string formatted for terminal display.
    ///
    /// # Arguments
    ///
    /// * `content` - The markdown content to be rendered
    pub fn render(&self, content: impl Into<String>) -> String {
        let content_string = content.into();

        // Strip excessive newlines before rendering
        let processed_content = self.strip_excessive_newlines(content_string.trim());

        self.skin
            .term_text(&processed_content)
            .to_string()
            .trim()
            .to_string()
    }

    /// Strip excessive consecutive newlines from content
    ///
    /// Reduces any sequence of more than max_consecutive_newlines to exactly
    /// max_consecutive_newlines
    fn strip_excessive_newlines(&self, content: &str) -> String {
        if content.is_empty() {
            return content.to_string();
        }

        let pattern = format!(r"\n{{{},}}", self.max_consecutive_newlines + 1);
        let re = Regex::new(&pattern).unwrap();
        let replacement = "\n".repeat(self.max_consecutive_newlines);

        re.replace_all(content, replacement.as_str()).to_string()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_render_simple_markdown() {
        let fixture = "# Test Heading\nThis is a test.";
        let markdown = MarkdownFormat::new();
        let actual = markdown.render(fixture);

        // Basic verification that output is non-empty
        assert!(!actual.is_empty());
    }

    #[test]
    fn test_render_empty_markdown() {
        let fixture = "";
        let markdown = MarkdownFormat::new();
        let actual = markdown.render(fixture);

        // Verify empty input produces empty output
        assert!(actual.is_empty());
    }

    #[test]
    fn test_strip_excessive_newlines_default() {
        let fixture = "Line 1\n\n\n\nLine 2";
        let formatter = MarkdownFormat::new();
        let actual = formatter.strip_excessive_newlines(fixture);
        let expected = "Line 1\n\nLine 2";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strip_excessive_newlines_custom() {
        let fixture = "Line 1\n\n\n\nLine 2";
        let formatter = MarkdownFormat::new().max_consecutive_newlines(3_usize);
        let actual = formatter.strip_excessive_newlines(fixture);
        let expected = "Line 1\n\n\nLine 2";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_with_excessive_newlines() {
        let fixture = "# Heading\n\n\n\nParagraph";
        let markdown = MarkdownFormat::new();

        // Use the default max_consecutive_newlines (2)
        let actual = markdown.render(fixture);

        // Compare with expected content containing only 2 newlines
        let expected = markdown.render("# Heading\n\nParagraph");

        // Strip any ANSI codes and whitespace for comparison
        let actual_clean = strip_ansi_escapes::strip_str(&actual).trim().to_string();
        let expected_clean = strip_ansi_escapes::strip_str(&expected).trim().to_string();

        assert_eq!(actual_clean, expected_clean);
    }

    #[test]
    fn test_render_with_custom_max_newlines() {
        let fixture = "# Heading\n\n\n\nParagraph";
        let markdown = MarkdownFormat::new().max_consecutive_newlines(1_usize);

        // Use a custom max_consecutive_newlines (1)
        let actual = markdown.render(fixture);

        // Compare with expected content containing only 1 newline
        let expected = markdown.render("# Heading\nParagraph");

        // Strip any ANSI codes and whitespace for comparison
        let actual_clean = strip_ansi_escapes::strip_str(&actual).trim().to_string();
        let expected_clean = strip_ansi_escapes::strip_str(&expected).trim().to_string();

        assert_eq!(actual_clean, expected_clean);
    }
}
