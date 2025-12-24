use std::sync::Arc;

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
    /// Supports both standard and indented code blocks (up to 3 spaces of
    /// indentation).
    pub fn new(content: &str) -> Self {
        let original_lines: Vec<&str> = content.lines().collect();
        let mut blocks = Vec::new();
        let mut result = String::new();
        let mut in_code = false;
        let mut code_lines: Vec<&str> = Vec::new();
        let mut lang = String::new();

        for line in &original_lines {
            // Check if line is a code fence (with or without indentation)
            if let Some(fence_lang) = Self::detect_code_fence(line) {
                if !in_code {
                    // Opening fence
                    lang = fence_lang;
                    in_code = true;
                } else {
                    // Closing fence
                    result.push_str(&format!("\x00{}\x00\n", blocks.len()));
                    blocks.push(CodeBlock { code: code_lines.join("\n"), lang: lang.clone() });
                    code_lines.clear();
                    in_code = false;
                }
            } else if in_code {
                // Inside code block - collect lines
                code_lines.push(line);
            } else {
                // Regular markdown line
                result.push_str(line);
                result.push('\n');
            }
        }

        Self { markdown: result, blocks }
    }

    /// Detect if a line is a code fence marker (```).
    /// Returns Some(language) if it's an opening fence with a language tag,
    /// Some("") if it's a fence without a language tag (opening or closing),
    /// None if it's not a code fence.
    fn detect_code_fence(line: &str) -> Option<String> {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            // Extract language tag (everything after ``` until whitespace or end)
            let lang = trimmed[3..].split_whitespace().next().unwrap_or("");
            Some(lang.to_string())
        } else {
            None
        }
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
        let r = CodeBlockParser::new("Hello world");
        assert!(r.markdown().contains("Hello world"));
        assert!(r.blocks().is_empty());
    }

    #[test]
    fn test_single_code_block() {
        let r = CodeBlockParser::new("```rust\nfn main() {}\n```");
        assert!(r.markdown().contains("\x000\x00"));
        assert_eq!(r.blocks().len(), 1);
        assert_eq!(r.blocks()[0].code, "fn main() {}");
        assert_eq!(r.blocks()[0].lang, "rust");
    }

    #[test]
    fn test_preserves_indentation() {
        let r = CodeBlockParser::new("```rust\n    let x = 1;\n```");
        assert_eq!(r.blocks()[0].code, "    let x = 1;");
    }

    #[test]
    fn test_restore() {
        let highlighter = SyntaxHighlighter::default();
        let r = CodeBlockParser::new("```rust\ncode\n```");
        let result = r.restore(&highlighter, "X\n\x000\x00\nY".into());
        assert!(strip_ansi(&result).contains("code"));
    }

    #[test]
    fn test_full_flow() {
        let highlighter = SyntaxHighlighter::default();
        let r = CodeBlockParser::new("Hi\n```rust\nlet x = 1;\n```\nBye");
        let result = strip_ansi(&r.restore(&highlighter, r.markdown().to_string()));
        assert!(result.contains("Hi") && result.contains("let x = 1") && result.contains("Bye"));
    }

    #[test]
    fn test_shared_highlighter() {
        let highlighter = SyntaxHighlighter::default();

        let r1 = CodeBlockParser::new("```rust\nlet x = 1;\n```");
        let r2 = CodeBlockParser::new("```python\nprint('hello')\n```");

        assert_eq!(r1.blocks()[0].lang, "rust");
        assert_eq!(r2.blocks()[0].lang, "python");

        let result1 = r1.restore(&highlighter, r1.markdown().to_string());
        let result2 = r2.restore(&highlighter, r2.markdown().to_string());

        assert!(strip_ansi(&result1).contains("let x = 1"));
        assert!(strip_ansi(&result2).contains("print('hello')"));
    }

    #[test]
    fn test_fixture_code_01() {
        // The fixture file code-01.md contains indented code blocks (with 3 spaces).
        // With the improved parser, these should now be properly extracted.
        let fixture = include_str!("fixtures/code-01.md");
        let highlighter = SyntaxHighlighter::default();

        let parser = CodeBlockParser::new(fixture);
        let actual_blocks = parser.blocks();

        // Verify that indented code blocks ARE extracted
        assert_eq!(actual_blocks.len(), 4, "Should extract all 4 code blocks");

        // Verify first code block
        assert_eq!(actual_blocks[0].lang, "rust");
        assert!(actual_blocks[0].code.contains("if env.enable_permissions"));

        // Verify second code block
        assert_eq!(actual_blocks[1].lang, "rust");
        assert!(actual_blocks[1].code.contains("ToolCatalog::Fetch(input)"));

        // Verify third code block
        assert_eq!(actual_blocks[2].lang, "rust");
        assert!(actual_blocks[2].code.contains("(Rule::Fetch(rule)"));

        // Verify fourth code block
        assert_eq!(actual_blocks[3].lang, "rust");
        assert!(actual_blocks[3].code.contains("ToolCatalog::Fetch(input)"));

        // Verify markdown contains placeholders
        let markdown = parser.markdown();
        assert!(markdown.contains("\x000\x00"));
        assert!(markdown.contains("\x001\x00"));
        assert!(markdown.contains("\x002\x00"));
        assert!(markdown.contains("\x003\x00"));

        // Verify full restoration flow preserves content
        let restored = parser.restore(&highlighter, markdown.to_string());
        let stripped = strip_ansi(&restored);
        assert!(stripped.contains("if env.enable_permissions"));
        assert!(stripped.contains("ToolCatalog::Fetch(input)"));
        assert!(stripped.contains("(Rule::Fetch(rule)"));
        assert!(stripped.contains("Permission Checking Flow"));
    }

    #[test]
    fn test_fixture_code_02() {
        let fixture = include_str!("fixtures/code-02.md");
        let highlighter = SyntaxHighlighter::default();

        let parser = CodeBlockParser::new(fixture);
        let actual_blocks = parser.blocks();

        // Verify correct number of code blocks extracted
        assert_eq!(actual_blocks.len(), 3);

        // Verify first code block (Rust)
        assert_eq!(actual_blocks[0].lang, "rust");
        assert!(actual_blocks[0].code.contains("fn main()"));
        assert!(actual_blocks[0].code.contains("println!"));

        // Verify second code block (Python)
        assert_eq!(actual_blocks[1].lang, "python");
        assert!(actual_blocks[1].code.contains("def greet"));

        // Verify third code block (JavaScript)
        assert_eq!(actual_blocks[2].lang, "javascript");
        assert!(actual_blocks[2].code.contains("function add"));

        // Verify markdown contains placeholders
        let markdown = parser.markdown();
        assert!(markdown.contains("\x000\x00"));
        assert!(markdown.contains("\x001\x00"));
        assert!(markdown.contains("\x002\x00"));

        // Verify full restoration flow preserves content
        let restored = parser.restore(&highlighter, markdown.to_string());
        let stripped = strip_ansi(&restored);
        assert!(stripped.contains("fn main()"));
        assert!(stripped.contains("def greet"));
        assert!(stripped.contains("function add"));
        assert!(stripped.contains("Sample Code Documentation"));
    }
}
