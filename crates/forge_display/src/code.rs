use std::sync::{Arc, Mutex};

use minimad::{Line, parse_text};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

/// Extracts code blocks, applies syntax highlighting, uses placeholders.
#[derive(Clone)]
pub struct MarkdownCodeRenderer {
    syntax_set: Arc<SyntaxSet>,
    theme_set: Arc<ThemeSet>,
    blocks: Arc<Mutex<Vec<String>>>,
}

impl Default for MarkdownCodeRenderer {
    fn default() -> Self {
        Self {
            syntax_set: Arc::new(SyntaxSet::load_defaults_newlines()),
            theme_set: Arc::new(ThemeSet::load_defaults()),
            blocks: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl MarkdownCodeRenderer {
    /// Extract code blocks, highlight them, return markdown with placeholders.
    pub fn process(&self, content: &str) -> String {
        let text = parse_text(content, minimad::Options::default().keep_code_fences(true));
        let original_lines: Vec<&str> = content.lines().collect();
        let mut blocks = self.blocks.lock().unwrap();
        blocks.clear();

        let mut result = String::new();
        let mut orig_idx = 0;
        let mut code_lines: Vec<&str> = Vec::new();
        let mut lang = "";
        let mut in_code = false;

        for line in &text.lines {
            match line {
                Line::CodeFence(c) if !in_code => {
                    lang = c.compounds.first().map(|c| c.src).unwrap_or("");
                    in_code = true;
                    orig_idx += 1;
                }
                Line::CodeFence(_) => {
                    result.push_str(&format!("\x00{}\x00\n", blocks.len()));
                    blocks.push(self.highlight(&code_lines.join("\n"), lang));
                    code_lines.clear();
                    in_code = false;
                    orig_idx += 1;
                }
                _ if in_code => {
                    if orig_idx < original_lines.len() {
                        code_lines.push(original_lines[orig_idx]);
                    }
                    orig_idx += 1;
                }
                _ => {
                    if orig_idx < original_lines.len() {
                        result.push_str(original_lines[orig_idx]);
                        result.push('\n');
                    }
                    orig_idx += 1;
                }
            }
        }
        result
    }

    /// Replace placeholders with highlighted code.
    pub fn restore(&self, mut rendered: String) -> String {
        let blocks = self.blocks.lock().unwrap();
        for (i, block) in blocks.iter().enumerate() {
            rendered = rendered.replace(&format!("\x00{i}\x00"), block);
        }
        rendered
    }

    fn highlight(&self, code: &str, lang: &str) -> String {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut hl = HighlightLines::new(syntax, theme);

        code.lines()
            .filter_map(|line| hl.highlight_line(line, &self.syntax_set).ok())
            .map(|ranges| format!("{}\x1b[0m", as_24_bit_terminal_escaped(&ranges, false)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        strip_ansi_escapes::strip_str(s).to_string()
    }

    #[test]
    fn test_no_code_blocks() {
        let r = MarkdownCodeRenderer::default();
        let md = r.process("Hello world");
        assert!(md.contains("Hello world"));
        assert!(r.blocks.lock().unwrap().is_empty());
    }

    #[test]
    fn test_single_code_block() {
        let r = MarkdownCodeRenderer::default();
        let md = r.process("```rust\nfn main() {}\n```");
        assert!(md.contains("\x000\x00"));
        assert_eq!(r.blocks.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_preserves_indentation() {
        let r = MarkdownCodeRenderer::default();
        r.process("```rust\n    let x = 1;\n```");
        let blocks = r.blocks.lock().unwrap();
        assert!(strip_ansi(&blocks[0]).contains("    let x = 1;"));
    }

    #[test]
    fn test_restore() {
        let r = MarkdownCodeRenderer::default();
        r.process("```rust\ncode\n```");
        let result = r.restore("X\n\x000\x00\nY".into());
        assert!(strip_ansi(&result).contains("code"));
    }

    #[test]
    fn test_full_flow() {
        let r = MarkdownCodeRenderer::default();
        let md = r.process("Hi\n```rust\nlet x = 1;\n```\nBye");
        let result = strip_ansi(&r.restore(md));
        assert!(result.contains("Hi") && result.contains("let x = 1") && result.contains("Bye"));
    }
}
