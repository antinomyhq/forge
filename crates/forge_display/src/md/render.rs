use derive_setters::Setters;
use lazy_regex::regex;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};
use termimad::crossterm::style::{Attribute, Color};
use termimad::crossterm::terminal;
use termimad::{Alignment, CompoundStyle, LineStyle, MadSkin};

#[derive(Debug)]
pub enum Segment {
    Text(String),
    Code(String),
}

#[derive(Setters)]
pub struct MarkdownRenderer {
    pub ss: SyntaxSet,
    pub theme: syntect::highlighting::Theme,
    pub width: usize,
    pub height: usize,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        let (width, height) = terminal::size().unwrap_or((80, 24));

        Self::new(
            (width as usize).saturating_sub(1),
            (height as usize).saturating_sub(1),
        )
    }
}

impl MarkdownRenderer {
    pub fn new(width: usize, height: usize) -> Self {
        let ss = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["Solarized (dark)"].clone();
        Self { ss, theme, width, height }
    }

    pub fn render(&self, content: &str, attr: Option<Attribute>) -> String {
        let skin = create_skin(attr);
        let segments = self.render_markdown(content);
        let mut result = String::new();
        for segment in segments {
            match segment {
                Segment::Text(t) => {
                    let rendered = skin.text(&t, Some(self.width));
                    result.push_str(&rendered.to_string());
                }
                Segment::Code(c) => {
                    result.push_str(&c);
                }
            }
        }
        result
    }

    fn render_markdown(&self, text: &str) -> Vec<Segment> {
        let re = regex!(r"(?ms)^```(\w+)?\n(.*?)(^```|\z)");
        let mut segments = vec![];
        let mut last_end = 0;

        for cap in re.captures_iter(text) {
            let start = cap.get(0).unwrap().start();
            if start > last_end {
                segments.push(Segment::Text(text[last_end..start].to_string()));
            }
            let lang = cap.get(1).map(|m| m.as_str()).unwrap_or("txt");

            let code = cap.get(2).unwrap().as_str();
            let wrapped_code = Self::wrap_code(code, self.width);
            let syntax = self
                .ss
                .find_syntax_by_token(lang)
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
        let mut result = "\n".to_string();
        for line in code.lines() {
            if line.chars().count() <= width {
                result.push_str(line);
                result.push('\n');
            } else {
                let mut current_line = String::new();
                let mut char_count = 0;

                for ch in line.chars() {
                    if char_count >= width {
                        result.push_str(&current_line);
                        result.push('\n');
                        current_line.clear();
                        char_count = 0;
                    }
                    current_line.push(ch);
                    char_count += 1;
                }

                if !current_line.is_empty() {
                    result.push_str(&current_line);
                    result.push('\n');
                }
            }
        }
        result
    }
}

fn create_skin(attr: Option<Attribute>) -> MadSkin {
    let mut skin = MadSkin::default();

    // Inline Code
    let style = CompoundStyle::new(Some(Color::Cyan), None, Attribute::Bold.into());
    skin.inline_code = style;

    // Code Blocks
    let codeblock_style = CompoundStyle::new(None, None, Default::default());
    skin.code_block = LineStyle::new(codeblock_style, Default::default());

    // Strikethrough
    let mut style = CompoundStyle::with_attr(Attribute::CrossedOut);
    style.add_attr(Attribute::Dim);
    skin.strikeout = style;

    // Headings
    let mut style = LineStyle::default();
    style.add_attr(Attribute::Bold);
    style.set_fg(Color::Green);

    let mut h1 = style.clone();
    h1.align = Alignment::Center;
    skin.headers = [
        h1,
        style.clone(),
        style.clone(),
        style.clone(),
        style.clone(),
        style.clone(),
        style.clone(),
        style.clone(),
    ];

    // Custom Attribute
    if let Some(attr) = attr {
        skin.paragraph.add_attr(attr);
        skin.inline_code.add_attr(attr);
        skin.code_block.compound_style.add_attr(attr);
        skin.strikeout.add_attr(attr);
    }

    skin
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use strip_ansi_escapes::strip_str;

    use super::*;

    #[test]
    fn test_wrap_code_long_line() {
        let fixture = "a".repeat(100);
        let actual = MarkdownRenderer::wrap_code(&fixture, 50);
        let expected = "a".repeat(50) + "\n" + &"a".repeat(50) + "\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_code_unicode_no_wrap() {
        // Test line with multi-byte chars where byte len > char count, but chars <=
        // width Old code: len()=5 > width=4, attempts wrapping, slices at byte
        // 4 splitting 'é' (invalid UTF-8) New code: chars().count()=4 <=4, no
        // wrap needed
        let fixture = "café"; // 4 chars, 5 bytes
        let actual = MarkdownRenderer::wrap_code(fixture, 4);
        let expected = "café\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_plain_text() {
        let fixture = MarkdownRenderer::new(80, 24);
        let input = "This is plain text.\n\nWith multiple lines.";
        let actual = fixture.render(input, None);
        let clean_actual = strip_str(&actual);
        assert!(clean_actual.contains("This is plain text."));
        assert!(clean_actual.contains("With multiple lines."));
    }

    #[test]
    fn test_render_multiple_code_blocks() {
        let fixture = MarkdownRenderer::new(80, 24);
        let input = "Text 1\n\n```\ncode1\n```\n\nText 2\n\n```\ncode2\n```\n\nText 3";
        let actual = fixture.render(input, None);
        let clean_actual = strip_str(&actual);
        assert!(clean_actual.contains("Text 1"));
        assert!(clean_actual.contains("code1"));
        assert!(clean_actual.contains("Text 2"));
        assert!(clean_actual.contains("code2"));
        assert!(clean_actual.contains("Text 3"));
        // Should have two reset codes for two code blocks
        let reset_count = actual.matches("\x1b[0m").count();
        assert_eq!(reset_count, 2);
    }

    #[test]
    fn test_render_unclosed_code_block() {
        let fixture = MarkdownRenderer::new(80, 24);
        let input = "Text\n\n```\nunclosed code";
        let actual = fixture.render(input, None);
        let clean_actual = strip_str(&actual);
        assert!(clean_actual.contains("Text"));
        assert!(clean_actual.contains("unclosed code"));
        assert!(actual.contains("\x1b[0m"));
    }

    #[test]
    fn test_segments_plain_text() {
        let fixture = MarkdownRenderer::new(80, 24);
        let input = "This is plain text.\n\nWith multiple lines.";
        let segments = fixture.render_markdown(input);
        assert_eq!(segments.len(), 1);
        assert!(matches!(segments[0], Segment::Text(ref t) if t == input));
    }

    #[test]
    fn test_segments_single_code_block_middle() {
        let fixture = MarkdownRenderer::new(80, 24);
        let input = "Before code.\n\n```\nfn main() {}\n```\n\nAfter code.";
        let segments = fixture.render_markdown(input);
        assert_eq!(segments.len(), 3);
        assert!(matches!(segments[0], Segment::Text(ref t) if t.contains("Before code.")));
        assert!(
            matches!(segments[1], Segment::Code(ref c) if strip_str(c).contains("fn main() {}"))
        );
        assert!(matches!(segments[2], Segment::Text(ref t) if t.contains("After code.")));
    }

    #[test]
    fn test_segments_multiple_code_blocks() {
        let fixture = MarkdownRenderer::new(80, 24);
        let input = "Text 1\n\n```\ncode1\n```\n\nText 2\n\n```\ncode2\n```\n\nText 3";
        let segments = fixture.render_markdown(input);
        assert_eq!(segments.len(), 5); // Text, Code, Text, Code, Text
        let code_count = segments
            .iter()
            .filter(|s| matches!(s, Segment::Code(_)))
            .count();
        assert_eq!(code_count, 2);
        let text_count = segments
            .iter()
            .filter(|s| matches!(s, Segment::Text(_)))
            .count();
        assert_eq!(text_count, 3);
    }
}
