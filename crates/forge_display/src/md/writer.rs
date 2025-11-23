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
impl Default for MarkdownWriter {
    fn default() -> Self {
        Self::new()
    }
}
impl MarkdownWriter {
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

        // +1 to consider the spinner
        let up_base = lines_prev.len().saturating_sub(common) + 1;
        if up_base > lines_to_update {
            skip = up_base - lines_to_update;
        }
        let up_lines = up_base.saturating_sub(skip);

        // Build ANSI sequence to write
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

        // Write above spinner; spinner will redraw itself
        let _ = spn.write_ln(out);
        self.previous_rendered = content.to_string();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use strip_ansi_escapes::strip_str;

    use super::*;

    #[test]
    fn test_markdown_writer_basic_incremental_update() {
        let mut spn = SpinnerManager::new();
        let previous_rendered = {
            let mut fixture = MarkdownWriter::new();
            fixture.stream("Line 1\nLine 2\nLine 3", &mut spn);
            fixture.previous_rendered.clone()
        };
        let expected = "Line 1\nLine 2\nLine 3";
        assert_eq!(previous_rendered, expected);
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
        let mut fixture = MarkdownWriter::new();
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
