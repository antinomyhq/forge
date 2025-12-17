use std::sync::Arc;

use minimad::{Line, parse_text};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

/// Loads and caches syntax highlighting resources.
#[derive(Clone)]
pub struct SyntaxHighlighter {
    syntax_set: Arc<SyntaxSet>,
    theme_set: Arc<ThemeSet>,
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self {
            syntax_set: Arc::new(SyntaxSet::load_defaults_newlines()),
            theme_set: Arc::new(ThemeSet::load_defaults()),
        }
    }
}

impl SyntaxHighlighter {
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

/// A code block extracted from markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeBlock {
    code: String,
    lang: String,
}

/// Holds extracted code blocks and processed markdown with placeholders.
#[derive(Clone)]
pub struct CodeBlockParser {
    markdown: String,
    blocks: Vec<CodeBlock>,
}

impl CodeBlockParser {
    /// Extract code blocks from markdown content.
    pub fn parse(content: &str) -> Self {
        let text = parse_text(content, minimad::Options::default().keep_code_fences(true));
        let original_lines: Vec<&str> = content.lines().collect();
        let mut blocks = Vec::new();

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
                    blocks.push(CodeBlock {
                        code: code_lines.join("\n"),
                        lang: lang.to_string(),
                    });
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

        Self { markdown: result, blocks }
    }

    /// Get the processed markdown with placeholders.
    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    /// Get the extracted code blocks.
    #[cfg(test)]
    pub fn blocks(&self) -> &[CodeBlock] {
        &self.blocks
    }

    /// Replace placeholders with highlighted code blocks.
    pub fn restore(&self, highlighter: &SyntaxHighlighter, mut rendered: String) -> String {
        for (i, block) in self.blocks.iter().enumerate() {
            let highlighted = highlighter.highlight(&block.code, &block.lang);
            rendered = rendered.replace(&format!("\x00{i}\x00"), &highlighted);
        }
        rendered
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
        let r = CodeBlockParser::parse("Hello world");
        assert!(r.markdown().contains("Hello world"));
        assert!(r.blocks().is_empty());
    }

    #[test]
    fn test_single_code_block() {
        let r = CodeBlockParser::parse("```rust\nfn main() {}\n```");
        assert!(r.markdown().contains("\x000\x00"));
        assert_eq!(r.blocks().len(), 1);
        assert_eq!(r.blocks()[0].code, "fn main() {}");
        assert_eq!(r.blocks()[0].lang, "rust");
    }

    #[test]
    fn test_preserves_indentation() {
        let r = CodeBlockParser::parse("```rust\n    let x = 1;\n```");
        assert_eq!(r.blocks()[0].code, "    let x = 1;");
    }

    #[test]
    fn test_restore() {
        let highlighter = SyntaxHighlighter::default();
        let r = CodeBlockParser::parse("```rust\ncode\n```");
        let result = r.restore(&highlighter, "X\n\x000\x00\nY".into());
        assert!(strip_ansi(&result).contains("code"));
    }

    #[test]
    fn test_full_flow() {
        let highlighter = SyntaxHighlighter::default();
        let r = CodeBlockParser::parse("Hi\n```rust\nlet x = 1;\n```\nBye");
        let result = strip_ansi(&r.restore(&highlighter, r.markdown().to_string()));
        assert!(result.contains("Hi") && result.contains("let x = 1") && result.contains("Bye"));
    }

    #[test]
    fn test_shared_highlighter() {
        let highlighter = SyntaxHighlighter::default();

        let r1 = CodeBlockParser::parse("```rust\nlet x = 1;\n```");
        let r2 = CodeBlockParser::parse("```python\nprint('hello')\n```");

        assert_eq!(r1.blocks()[0].lang, "rust");
        assert_eq!(r2.blocks()[0].lang, "python");

        let result1 = r1.restore(&highlighter, r1.markdown().to_string());
        let result2 = r2.restore(&highlighter, r2.markdown().to_string());

        assert!(strip_ansi(&result1).contains("let x = 1"));
        assert!(strip_ansi(&result2).contains("print('hello')"));
    }
}
