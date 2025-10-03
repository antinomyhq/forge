use lazy_regex::regex;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};
use termimad::crossterm::style::{Attribute, Color};
use termimad::crossterm::terminal;
use termimad::{CompoundStyle, LineStyle, MadSkin};

#[derive(Debug)]
pub enum Segment {
    Text(String),
    Code(String),
}

pub struct MarkdownRenderer {
    skin: MadSkin,
    ss: SyntaxSet,
    theme: syntect::highlighting::Theme,
    width: usize,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        let (width, _) = terminal::size().unwrap_or((80, 24));
        let mut skin = MadSkin::default();
        let compound_style = CompoundStyle::new(Some(Color::Cyan), None, Attribute::Bold.into());
        skin.inline_code = compound_style.clone();

        let codeblock_style = CompoundStyle::new(None, None, Default::default());
        skin.code_block = LineStyle::new(codeblock_style, Default::default());

        let mut strikethrough_style = CompoundStyle::with_attr(Attribute::CrossedOut);
        strikethrough_style.add_attr(Attribute::Dim);
        skin.strikeout = strikethrough_style;

        Self::new(skin, (width as usize).saturating_sub(1))
    }
}

impl MarkdownRenderer {
    pub fn new(skin: MadSkin, width: usize) -> Self {
        let ss = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["Solarized (dark)"].clone();
        Self { skin, ss, theme, width }
    }

    pub fn render(&self, content: &str) -> String {
        let segments = self.render_markdown(content);
        let mut result = String::new();
        for segment in segments {
            match segment {
                Segment::Text(t) => {
                    let rendered = self.skin.text(&t, Some(self.width));
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
        let mut result = String::new();
        for line in code.lines() {
            if line.len() <= width {
                result.push_str(line);
                result.push('\n');
            } else {
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
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use strip_ansi_escapes::strip_str;

    use super::*;

    #[test]
    fn test_render_plain_text() {
        let fixture = MarkdownRenderer::new(MadSkin::default(), 80);
        let actual = fixture.render("Hello world");
        let expected = fixture.skin.text("Hello world", Some(80)).to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_code_block() {
        let fixture = MarkdownRenderer::new(MadSkin::default(), 80);
        let actual = fixture.render("```\nfn main() {}\n```");
        // Since code highlighting is complex, just check it contains the code
        assert!(actual.contains("fn main() {}"));
    }

    #[test]
    fn test_wrap_code_short_lines() {
        let fixture = "line1\nline2";
        let actual = MarkdownRenderer::wrap_code(fixture, 80);
        let expected = "line1\nline2\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_code_long_line() {
        let fixture = "a".repeat(100);
        let actual = MarkdownRenderer::wrap_code(&fixture, 50);
        let expected = "a".repeat(50) + "\n" + &"a".repeat(50) + "\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_markdown_writer_add_chunk_and_flush() {
        let renderer = MarkdownRenderer::new(MadSkin::default(), 80);
        let mut fixture = MarkdownWriter::new(renderer);
        fixture.add_chunk("Hello");
        let actual = fixture.flush();
        assert!(actual.is_some());
        assert!(actual.unwrap().contains("Hello"));
    }

    #[test]
    fn test_markdown_writer_long_text_chunk_by_chunk() {
        let renderer = MarkdownRenderer::new(MadSkin::default(), 80);
        let mut fixture = MarkdownWriter::new(renderer);

        let long_text = r#"# Header

This is a long paragraph with multiple sentences. It contains various types of content including some code examples.

```rust
fn main() {
    println!("Hello, world!");
    let x = 42;
    println!("The answer is {}", x);
}
```

And some more text after the code block."#;

        // Split into chunks and add with spaces
        let chunks = long_text.split_whitespace().collect::<Vec<_>>();
        for chunk in chunks {
            fixture.add_chunk(&format!("{} ", chunk));
        }

        let actual = fixture.flush();
        assert!(actual.is_some());
        let output = actual.unwrap();
        // Remove ANSI codes for easier testing
        let clean_output = strip_str(&output);
        assert!(clean_output.contains("Header"));
        assert!(clean_output.contains("println!"));
        assert!(clean_output.contains("Hello, world!"));
        assert!(clean_output.contains("more text"));
    }
}

pub struct MarkdownWriter {
    buffer: String,
    renderer: MarkdownRenderer,
    previous_rendered: String,
}

impl MarkdownWriter {
    pub fn new(renderer: MarkdownRenderer) -> Self {
        Self {
            buffer: String::new(),
            renderer,
            previous_rendered: String::new(),
        }
    }

    pub fn add_chunk(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);

        if let Some(rendered) = self.try_render() {
            self.stream(&rendered);
        }
    }

    fn try_render(&mut self) -> Option<String> {
        let result = self.renderer.render(&self.buffer);
        Some(result)
    }

    pub fn flush(&mut self) -> Option<String> {
        if !self.buffer.is_empty() {
            let result = self.renderer.render(&self.buffer);
            self.buffer.clear();
            self.previous_rendered.clear();
            Some(result)
        } else {
            None
        }
    }

    pub fn stream(&mut self, content: &str) {
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
