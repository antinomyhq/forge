use derive_setters::Setters;
use lazy_regex::regex;
use regex::Regex;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};
use termimad::crossterm::style::{Attribute, Color};
use termimad::{CompoundStyle, LineStyle, MadSkin};

#[derive(Debug)]
pub enum Segment {
    Text(String),
    Code(String),
}

/// MarkdownFormat provides functionality for formatting markdown text for
/// terminal display.
#[derive(Clone, Setters, Default)]
#[setters(into, strip_option)]
pub struct MarkdownFormat {
    skin: MadSkin,
    max_consecutive_newlines: usize,
    width: usize,
}

impl MarkdownFormat {
    /// Create a new MarkdownFormat with the default skin
    pub fn new() -> Self {
        let mut skin = MadSkin::default();
        let compound_style = CompoundStyle::new(Some(Color::Cyan), None, Attribute::Bold.into());
        skin.inline_code = compound_style.clone();

        let codeblock_style = CompoundStyle::new(None, None, Default::default());
        skin.code_block = LineStyle::new(codeblock_style, Default::default());

        let mut strikethrough_style = CompoundStyle::with_attr(Attribute::CrossedOut);
        strikethrough_style.add_attr(Attribute::Dim);
        skin.strikeout = strikethrough_style;

        Self { skin, max_consecutive_newlines: 2, width: 80 }
    }

    /// Render the markdown content to a string formatted for terminal display.
    ///
    /// # Arguments
    ///
    /// * `content` - The markdown content to be rendered
    pub fn render(&self, content: impl Into<String>) -> String {
        let content_string = content.into();

        // Strip excessive newlines before rendering
        let processed_content = self.strip_excessive_newlines(&content_string);

        self.skin.term_text(&processed_content).to_string()
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

    /// Creates a streaming markdown processor.
    pub fn writer(&self) -> MarkdownWriter {
        MarkdownWriter::new(self.skin.clone(), self.width)
    }
}

#[derive(Clone)]
pub struct MarkdownWriter {
    buffer: String,
    skin: MadSkin,
    ss: SyntaxSet,
    theme: syntect::highlighting::Theme,
    previous_rendered: String,
    width: usize,
}

impl MarkdownWriter {
    /// Creates a new streaming markdown writer.
    pub fn new(skin: MadSkin, width: usize) -> Self {
        let ss = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["Solarized (dark)"].clone();
        Self {
            buffer: String::new(),
            skin,
            ss,
            theme,
            previous_rendered: String::new(),
            width,
        }
    }

    /// Adds a chunk of text by adding each character.
    pub fn add_chunk(&mut self, chunk: &str) -> std::io::Result<Option<String>> {
        for c in chunk.chars() {
            self.add_char(c)?;
        }
        self.try_render()
    }

    /// Adds a single character and renders immediately.
    pub fn add_char(&mut self, c: char) -> std::io::Result<()> {
        self.buffer.push(c);
        if let Some(rendered) = self.try_render()? {
            self.stream_rendered(&rendered);
        }
        Ok(())
    }

    fn try_render(&mut self) -> std::io::Result<Option<String>> {
        let segments = self.render_markdown(&self.buffer);
        let mut result = String::new();
        for segment in segments {
            match segment {
                Segment::Text(text) => {
                    let rendered = self.skin.text(&text, Some(self.width));
                    result.push_str(&rendered.to_string());
                }
                Segment::Code(code) => {
                    result.push_str(&code);
                }
            }
        }
        Ok(Some(result))
    }

    /// Renders and returns any remaining content from the buffer.
    pub fn flush(&mut self) -> std::io::Result<Option<String>> {
        if !self.buffer.is_empty() {
            let segments = self.render_markdown(&self.buffer);
            let mut result = String::new();
            for segment in segments {
                match segment {
                    Segment::Text(text) => {
                        let rendered = self.skin.text(&text, Some(self.width));
                        result.push_str(&rendered.to_string());
                    }
                    Segment::Code(code) => {
                        result.push_str(&code);
                    }
                }
            }
            self.buffer.clear();
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    fn render_markdown(&self, text: &str) -> Vec<Segment> {
        let re = regex!(r"(?s)```(\w+)?\n(.*?)(```|\z)");
        let mut segments = vec![];
        let mut last_end = 0;
        for cap in re.captures_iter(text) {
            let start = cap.get(0).unwrap().start();
            if start > last_end {
                segments.push(Segment::Text(text[last_end..start].to_string()));
            }
            let lang = cap.get(1).map(|m| m.as_str()).unwrap_or("txt");
            let ext = match lang {
                "rust" => "rs",
                "javascript" => "js",
                "python" => "py",
                _ => lang,
            };
            let code = cap.get(2).unwrap().as_str();
            let wrapped_code = Self::wrap_code(code, self.width);
            let syntax = self
                .ss
                .find_syntax_by_extension(ext)
                .unwrap_or_else(|| self.ss.find_syntax_plain_text());
            let mut h = HighlightLines::new(syntax, &self.theme);
            let mut highlighted = String::new();
            for line in LinesWithEndings::from(&wrapped_code) {
                let ranges: Vec<(syntect::highlighting::Style, &str)> =
                    h.highlight_line(line, &self.ss).unwrap();
                highlighted.push_str(&as_24_bit_terminal_escaped(&ranges[..], false));
            }
            highlighted.push_str("\x1b[0m");
            segments.push(Segment::Code(highlighted));
            last_end = cap.get(0).unwrap().end();
        }
        if last_end < text.len() {
            segments.push(Segment::Text(text[last_end..].to_string()));
        }
        segments
    }

    fn wrap_code(code: &str, width: usize) -> String {
        let mut result = String::new();
        for line in code.lines() {
            if line.len() <= width {
                result.push_str(line);
                result.push('\n');
            } else {
                // Simple wrap at width, but since it's code, just split
                let mut start = 0;
                while start < line.len() {
                    let end = (start + width).min(line.len());
                    result.push_str(&line[start..end]);
                    result.push('\n');
                    start = end;
                }
            }
        }
        result
    }

    /// Streams the rendered content with ANSI clearing like markdown_renderer.
    pub fn stream_rendered(&mut self, content: &str) {
        let rendered_lines: Vec<&str> = content.lines().collect();
        let lines_new: Vec<&str> = rendered_lines;
        let lines_prev: Vec<&str> = self.previous_rendered.lines().collect();
        let common = lines_prev
            .iter()
            .zip(&lines_new)
            .take_while(|(p, n)| p == n)
            .count();
        if common < lines_prev.len() {
            let up_lines = lines_prev.len() - common;
            if up_lines > 0 {
                print!("\x1b[{}A", up_lines);
            }
            print!("\x1b[0J");
        }
        for line in &lines_new[common..] {
            println!("{}\x1b[K", line);
        }
        self.previous_rendered = content.to_string();
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

    #[test]
    fn test_markdown_writer() {
        let fixture = "# Test Heading\n\nThis is a paragraph.";
        let mut writer = MarkdownWriter::new(MadSkin::default(), 80);

        let result = writer.add_chunk(fixture);
        assert!(result.is_ok());

        let flush_result = writer.flush();
        assert!(flush_result.is_ok());

        // Collect all output
        let mut output = String::new();
        if let Some(rendered) = result.unwrap() {
            output.push_str(&rendered);
        }
        if let Some(remaining) = flush_result.unwrap() {
            output.push_str(&remaining);
        }

        let actual_clean = strip_ansi_escapes::strip_str(&output).trim().to_string();

        // Expected output should contain the heading and paragraph
        assert!(actual_clean.contains("Test Heading"));
        assert!(actual_clean.contains("This is a paragraph."));
    }

    #[test]
    fn test_markdown_writer_code_block() {
        let fixture = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let mut writer = MarkdownWriter::new(MadSkin::default(), 80);

        let result = writer.add_chunk(fixture);
        assert!(result.is_ok());

        let flush_result = writer.flush();
        assert!(flush_result.is_ok());

        // Collect all output
        let mut output = String::new();
        if let Some(rendered) = result.unwrap() {
            output.push_str(&rendered);
        }
        if let Some(remaining) = flush_result.unwrap() {
            output.push_str(&remaining);
        }

        let actual_clean = strip_ansi_escapes::strip_str(&output).trim().to_string();

        // Expected output should contain the code
        assert!(actual_clean.contains("fn main()"));
        assert!(actual_clean.contains("println!(\"Hello\")"));
    }
}
