use std::fmt::Display;

/// A builder for constructing Markdown documents.
/// Provides a fluent API for creating structured Markdown content.
#[derive(Default, Clone)]
pub struct Markdown {
    parts: Vec<String>,
}

impl Markdown {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a heading at the specified level (1-6)
    pub fn heading(mut self, level: u8, text: impl ToString) -> Self {
        let level = level.min(6).max(1);
        self.parts.push(format!("{} {}", "#".repeat(level as usize), text.to_string()));
        self.parts.push(String::new()); // Add blank line after heading
        self
    }

    /// Adds an H1 heading
    pub fn h1(self, text: impl ToString) -> Self {
        self.heading(1, text)
    }

    /// Adds an H2 heading
    pub fn h2(self, text: impl ToString) -> Self {
        self.heading(2, text)
    }

    /// Adds an H3 heading
    pub fn h3(self, text: impl ToString) -> Self {
        self.heading(3, text)
    }

    /// Adds bold text
    pub fn bold(mut self, text: impl ToString) -> Self {
        self.parts.push(format!("**{}**", text.to_string()));
        self
    }

    /// Adds italic text
    pub fn italic(mut self, text: impl ToString) -> Self {
        self.parts.push(format!("*{}*", text.to_string()));
        self
    }

    /// Adds inline code
    pub fn code_inline(mut self, text: impl ToString) -> Self {
        self.parts.push(format!("`{}`", text.to_string()));
        self
    }

    /// Adds a code block with optional language
    pub fn code_block(mut self, code: impl ToString, language: Option<&str>) -> Self {
        if let Some(lang) = language {
            self.parts.push(format!("```{}", lang));
        } else {
            self.parts.push("```".to_string());
        }
        self.parts.push(code.to_string());
        self.parts.push("```".to_string());
        self.parts.push(String::new()); // Add blank line after code block
        self
    }

    /// Adds a plain text paragraph
    pub fn text(mut self, text: impl ToString) -> Self {
        self.parts.push(text.to_string());
        self
    }

    /// Adds a line (text followed by newline)
    pub fn line(mut self, text: impl ToString) -> Self {
        self.parts.push(text.to_string());
        self.parts.push(String::new());
        self
    }

    /// Adds a blank line
    pub fn blank_line(mut self) -> Self {
        self.parts.push(String::new());
        self
    }

    /// Adds a bullet list item
    pub fn bullet(mut self, text: impl ToString) -> Self {
        self.parts.push(format!("- {}", text.to_string()));
        self
    }

    /// Adds a numbered list item
    pub fn numbered(mut self, number: usize, text: impl ToString) -> Self {
        self.parts.push(format!("{}. {}", number, text.to_string()));
        self
    }

    /// Adds a blockquote
    pub fn quote(mut self, text: impl ToString) -> Self {
        self.parts.push(format!("> {}", text.to_string()));
        self
    }

    /// Adds a horizontal rule
    pub fn hr(mut self) -> Self {
        self.parts.push("---".to_string());
        self.parts.push(String::new());
        self
    }

    /// Adds a link
    pub fn link(mut self, text: impl ToString, url: impl ToString) -> Self {
        self.parts.push(format!("[{}]({})", text.to_string(), url.to_string()));
        self
    }

    /// Adds raw markdown content
    pub fn raw(mut self, markdown: impl ToString) -> Self {
        self.parts.push(markdown.to_string());
        self
    }

    /// Adds a key-value pair formatted as "- **key:** value"
    pub fn kv(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.parts.push(format!("- **{}:** {}", key.to_string(), value.to_string()));
        self
    }

    /// Adds a key-value pair with the value as inline code
    pub fn kv_code(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.parts.push(format!("- **{}:** `{}`", key.to_string(), value.to_string()));
        self
    }

    /// Conditionally adds content if the condition is true
    pub fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if condition {
            f(self)
        } else {
            self
        }
    }

    /// Conditionally adds content if the option is Some
    pub fn when_some<T>(self, option: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        if let Some(value) = option {
            f(self, value)
        } else {
            self
        }
    }

    /// Renders the Markdown to a String
    pub fn render(&self) -> String {
        self.parts.join("\n")
    }
}

impl Display for Markdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

impl From<Markdown> for String {
    fn from(md: Markdown) -> Self {
        md.render()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_basic_markdown() {
        let md = Markdown::new()
            .h1("Title")
            .text("Some text")
            .blank_line()
            .bullet("Item 1")
            .bullet("Item 2");

        let expected = "# Title\n\nSome text\n\n- Item 1\n- Item 2";
        assert_eq!(md.render(), expected);
    }

    #[test]
    fn test_code_block() {
        let md = Markdown::new()
            .h2("Code Example")
            .code_block("fn main() {}", Some("rust"));

        let expected = "## Code Example\n\n```rust\nfn main() {}\n```\n";
        assert_eq!(md.render(), expected);
    }

    #[test]
    fn test_key_value() {
        let md = Markdown::new()
            .kv("Pattern", "*.rs")
            .kv_code("Path", "/home/user");

        let expected = "- **Pattern:** *.rs\n- **Path:** `/home/user`";
        assert_eq!(md.render(), expected);
    }

    #[test]
    fn test_conditional() {
        let md = Markdown::new()
            .text("Always shown")
            .when(true, |md| md.text(" - shown"))
            .when(false, |md| md.text(" - hidden"));

        let expected = "Always shown\n - shown";
        assert_eq!(md.render(), expected);
    }

    #[test]
    fn test_conditional_some() {
        let md = Markdown::new()
            .text("Base")
            .when_some(Some("value"), |md, v| md.text(format!(" - {}", v)))
            .when_some(None::<String>, |md, v| md.text(format!(" - {}", v)));

        let expected = "Base\n - value";
        assert_eq!(md.render(), expected);
    }
}
