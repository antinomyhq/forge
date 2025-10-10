use crossterm::cursor::MoveUp;
use crossterm::execute;
use crossterm::terminal::Clear;
use termimad::crossterm::style::Attribute;

use crate::md::render::MarkdownRenderer;

pub struct MarkdownWriter<W> {
    buffer: String,
    renderer: MarkdownRenderer,
    previous_rendered: String,
    writer: W,
    last_was_dimmed: bool,
}

impl<W> MarkdownWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            buffer: String::new(),
            renderer: MarkdownRenderer::default(),
            previous_rendered: String::new(),
            writer,
            last_was_dimmed: false,
        }
    }
}

impl<W: std::io::Write> MarkdownWriter<W> {
    #[cfg(test)]
    fn with_renderer(mut self, renderer: MarkdownRenderer) -> Self {
        self.renderer = renderer;
        self
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.previous_rendered.clear();
    }

    pub fn add_chunk(&mut self, chunk: &str) {
        if self.last_was_dimmed {
            self.reset();
        }
        self.buffer.push_str(chunk);
        self.stream(&self.renderer.render(&self.buffer, None));
        self.last_was_dimmed = false;
    }

    pub fn add_chunk_dimmed(&mut self, chunk: &str) {
        if !self.last_was_dimmed {
            self.reset();
        }
        self.buffer.push_str(chunk);
        self.stream(&self.renderer.render(&self.buffer, Some(Attribute::Dim)));
        self.last_was_dimmed = true;
    }

    fn stream(&mut self, content: &str) {
        let rendered_lines: Vec<&str> = content.lines().collect();
        let lines_new: Vec<&str> = rendered_lines;
        let lines_prev: Vec<&str> = self.previous_rendered.lines().collect();
        let common = lines_prev
            .iter()
            .zip(&lines_new)
            .take_while(|(p, n)| p == n)
            .count();

        let lines_to_update = self.renderer.height;
        let mut skip = 0;
        let up_lines = lines_prev.len() - common;

        if up_lines > lines_to_update {
            skip = up_lines - lines_to_update;
        }
        let up_lines = (lines_prev.len() - common) - skip;
        if up_lines > 0 {
            execute!(self.writer, MoveUp(up_lines as u16)).unwrap();
        }
        execute!(
            self.writer,
            Clear(crossterm::terminal::ClearType::FromCursorDown)
        )
        .unwrap();
        for line in lines_new[common + skip..].iter() {
            writeln!(self.writer, "{}", line).unwrap();
        }
        self.writer.flush().unwrap();
        self.previous_rendered = content.to_string();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pretty_assertions::assert_eq;
    use strip_ansi_escapes::strip_str;

    use super::*;

    #[test]
    fn test_markdown_writer_basic_incremental_update() {
        let mut output = Vec::new();
        let previous_rendered = {
            let mut writer = MarkdownWriter::new(Box::new(Cursor::new(&mut output)));
            writer.stream("Line 1\nLine 2\nLine 3");
            writer.previous_rendered.clone()
        };
        assert_eq!(previous_rendered, "Line 1\nLine 2\nLine 3");
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Line 1"));
        assert!(output_str.contains("Line 2"));
        assert!(output_str.contains("Line 3"));
    }

    #[test]
    fn test_markdown_writer_full_clear_with_height_cap() {
        let renderer = MarkdownRenderer::new(80, 2);
        let mut output = Vec::new();
        {
            let mut writer = MarkdownWriter::new(Cursor::new(&mut output)).with_renderer(renderer);
            writer.previous_rendered = "Old 1\nOld 2\nOld 3\nOld 4\nOld 5".to_string();
            writer.stream("new 1\nnew 2\nnew3\nnew 4\n new 5\n new6");
        }
        let output_str = String::from_utf8(output).unwrap();
        // common=0, up_lines=5, height=2, skip=3, up_lines=2, print \x1b[2A \x1b[0J
        // New\n (take 2, but only 1 line)
        assert!(output_str.contains("\x1b[2A"));
        assert!(output_str.contains("\x1b[0J"));
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
    fn test_markdown_writer_long_text_chunk_by_chunk() {
        let mut fixture = MarkdownWriter::new(Box::new(std::io::sink()));

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

        assert!(fixture.buffer.contains("Header"));
        assert!(fixture.buffer.contains("println!"));
        assert!(fixture.buffer.contains("Hello, world!"));
        assert!(fixture.buffer.contains("more text"));
    }
}
