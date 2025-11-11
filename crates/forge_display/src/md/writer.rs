use forge_spinner::SpinnerManager;
use termimad::crossterm::style::Attribute;

use crate::md::render::MarkdownRenderer;

pub struct MarkdownWriter {
    buffer: String,
    renderer: MarkdownRenderer,
    previous_rendered: String,
    last_was_dimmed: bool,
}

impl MarkdownWriter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            renderer: MarkdownRenderer::default(),
            previous_rendered: String::new(),
            last_was_dimmed: false,
        }
    }
}

impl MarkdownWriter {
    #[cfg(test)]
    fn with_renderer(mut self, renderer: MarkdownRenderer) -> Self {
        self.renderer = renderer;
        self
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.previous_rendered.clear();
    }

    pub fn add_chunk(&mut self, chunk: &str, spn: &mut SpinnerManager) {
        if self.last_was_dimmed {
            self.reset();
        }
        self.buffer.push_str(chunk);
        self.stream(&self.renderer.render(&self.buffer, None), spn);
        self.last_was_dimmed = false;
    }

    pub fn add_chunk_dimmed(&mut self, chunk: &str, spn: &mut SpinnerManager) {
        if !self.last_was_dimmed {
            self.reset();
        }
        self.buffer.push_str(chunk);
        self.stream(
            &self.renderer.render(&self.buffer, Some(Attribute::Dim)),
            spn,
        );
        self.last_was_dimmed = true;
    }

    fn stream(&mut self, content: &str, spn: &mut SpinnerManager) {
        let lines_new: Vec<&str> = content.lines().collect();
        let lines_prev: Vec<String> = self
            .previous_rendered
            .lines()
            .map(|s| s.to_string())
            .collect();

        // Compute common prefix to minimize redraw
        let common = lines_prev
            .iter()
            .map(|s| s.as_str())
            .zip(&lines_new)
            .take_while(|(p, n)| p == *n)
            .count();

        let lines_to_update = self.renderer.height;
        let mut skip = 0;
        let up_base = lines_prev.len().saturating_sub(common);
        if up_base > lines_to_update {
            skip = up_base - lines_to_update;
        }
        let up_lines = up_base.saturating_sub(skip) + 1; // +1 to account for spinner line

        // Build ANSI sequence payload to write via spinner API
        let mut out = String::new();
        if up_lines > 0 {
            out.push_str(&format!("\x1b[{}A", up_lines)); // move up
        }
        out.push_str("\x1b[0J"); // clear from cursor down
        for line in lines_new.iter().skip(common + skip) {
            out.push_str(line);
            out.push('\n');
            out.push_str("\x1b[0G"); // move to column 0
        }
        out.push_str("\r"); // return carriage; spinner will add newline

        // Write above spinner; spinner will redraw itself
        let _ = spn.write_ln(out);

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
        let mut spn = SpinnerManager::new();
        let previous_rendered = {
            let mut writer = MarkdownWriter::new(Box::new(Cursor::new(&mut output)));
            writer.stream("Line 1\nLine 2\nLine 3", &mut spn);
            writer.previous_rendered.clone()
        };
        assert_eq!(previous_rendered, "Line 1\nLine 2\nLine 3");
        let output_str = String::from_utf8(output).unwrap();
        panic!("error {}", output_str);
        assert!(output_str.contains("Line 1"));
        assert!(output_str.contains("Line 2"));
        assert!(output_str.contains("Line 3"));
    }

    #[test]
    fn test_markdown_writer_full_clear_with_height_cap() {
        let renderer = MarkdownRenderer::new(80, 2);
        let mut output = Vec::new();
        let mut spn = SpinnerManager::new();
        {
            let mut writer = MarkdownWriter::new(Cursor::new(&mut output)).with_renderer(renderer);
            writer.previous_rendered = "Old 1\nOld 2\nOld 3\nOld 4\nOld 5".to_string();
            writer.stream("new 1\nnew 2\nnew3\nnew 4\n new 5\n new6", &mut spn);
        }
        let output_str = String::from_utf8(output).unwrap();
        // common=0, up_lines=5, height=2, skip=3, up_lines=2, print \x1b[2A \x1b[0J
        // New\n (take 2, but only 1 line) + 1 (for spinner is skips one line)
        assert!(output_str.contains("\x1b[2A"));
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
        let mut spn = SpinnerManager::new();

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
            fixture.add_chunk(&format!("{} ", chunk), &mut spn);
        }

        assert!(fixture.buffer.contains("Header"));
        assert!(fixture.buffer.contains("println!"));
        assert!(fixture.buffer.contains("Hello, world!"));
        assert!(fixture.buffer.contains("more text"));
    }
}
