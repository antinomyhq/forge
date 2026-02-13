use std::fmt::Display;

/// A format-agnostic document builder that stores raw values and renders on demand.
/// 
/// Unlike `Element` which formats strings immediately, `Document` defers formatting
/// until `render_xml()` or `render_markdown()` is called. This allows the same
/// structure to be rendered in multiple formats.
/// 
/// # Semantic Tags
/// 
/// Document supports semantic tags that render differently in XML vs Markdown:
/// - `bold` -> `<bold>text</bold>` (XML), `**text**` (Markdown)
/// - `italic` -> `<italic>text</italic>` (XML), `*text*` (Markdown)
/// - `code` -> `<code>text</code>` (XML), `` `text` `` (Markdown)
/// - `h1`-`h6` -> `<h1>text</h1>` (XML), `# text` (Markdown)
/// - `list` -> `<list>` (XML), bullet list (Markdown)
/// - `item` -> `<item>` (XML), `- text` (Markdown)
/// - `code_block` -> `<code_block>` with CDATA (XML), ` ```lang\ncode\n``` ` (Markdown)
/// 
/// # Examples
/// 
/// ```
/// use forge_template::Document;
/// 
/// let doc = Document::new("bold").text("Important");
/// assert_eq!(doc.render_xml(), "<bold>Important</bold>");
/// assert_eq!(doc.render_markdown(), "**Important**");
/// ```
#[derive(Clone, Debug)]
pub struct Document {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Document>,
    pub content: Option<Content>,
}

/// Content types for Document nodes.
#[derive(Clone, Debug)]
pub enum Content {
    /// Plain text that will be escaped in XML, rendered as-is in Markdown
    Text(String),
    /// Raw content (CDATA in XML, code block in Markdown)
    Raw(String),
}

impl Document {
    /// Creates a new Document with the given tag name.
    /// 
    /// Supports CSS-style class syntax: `Document::new("div.foo.bar")`
    /// will create a div with `class="foo bar"`.
    pub fn new(tag_with_classes: impl ToString) -> Self {
        let full_tag = tag_with_classes.to_string();
        let parts: Vec<&str> = full_tag.split('.').collect();

        let mut doc = Document {
            tag: parts[0].to_string(),
            attrs: vec![],
            children: vec![],
            content: None,
        };

        // Add classes if there are any
        if parts.len() > 1 {
            let classes = parts[1..].join(" ");
            doc.attrs.push(("class".to_string(), classes));
        }

        doc
    }

    /// Adds an attribute to the document.
    pub fn attr(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.attrs.push((key.to_string(), value.to_string()));
        self
    }

    /// Conditionally adds an attribute if the value is Some.
    pub fn attr_if_some(mut self, key: impl ToString, value: Option<impl ToString>) -> Self {
        if let Some(val) = value {
            self.attrs.push((key.to_string(), val.to_string()));
        }
        self
    }

    /// Adds a CSS class to the document.
    pub fn class(mut self, class_name: impl ToString) -> Self {
        // Check if class attribute already exists
        if let Some(pos) = self.attrs.iter().position(|(key, _)| key == "class") {
            // Append to existing class
            let (_, current_class) = &self.attrs[pos];
            let new_class = format!("{} {}", current_class, class_name.to_string());
            self.attrs[pos] = ("class".to_string(), new_class);
        } else {
            // Add new class attribute
            self.attrs
                .push(("class".to_string(), class_name.to_string()));
        }
        self
    }

    /// Sets the text content (will be escaped in XML).
    pub fn text(mut self, text: impl ToString) -> Self {
        self.content = Some(Content::Text(text.to_string()));
        self
    }

    /// Sets raw content (CDATA in XML, code block in Markdown).
    pub fn cdata(mut self, text: impl ToString) -> Self {
        self.content = Some(Content::Raw(text.to_string()));
        self
    }

    /// Appends children to the document.
    pub fn append(self, item: impl CanAppendDoc) -> Self {
        item.append_to_doc(self)
    }

    /// Creates a span element with text content.
    pub fn span(text: impl ToString) -> Self {
        Document::new("span").text(text)
    }

    /// Renders the document as XML.
    pub fn render_xml(&self) -> String {
        let mut result = String::new();

        if self.attrs.is_empty() {
            result.push_str(&format!("<{}>", self.tag));
        } else {
            result.push_str(&format!("<{}", self.tag));
            for (key, value) in &self.attrs {
                result.push_str(&format!("\n  {key}=\"{value}\""));
            }
            result.push_str("\n>");
        }

        if let Some(ref content) = self.content {
            match content {
                Content::Text(text) => {
                    result.push_str(&html_escape::encode_text(text).to_string());
                }
                Content::Raw(raw) => {
                    result.push_str(&format!("<![CDATA[{}]]>", raw));
                }
            }
        }

        for child in &self.children {
            result.push('\n');
            result.push_str(&child.render_xml());
        }

        if self.children.is_empty() && self.attrs.is_empty() {
            result.push_str(&format!("</{}>", self.tag));
        } else {
            result.push_str(&format!("\n</{}>", self.tag));
        }

        result
    }

    /// Renders the document as Markdown.
    /// 
    /// Interprets semantic tags and converts them to Markdown syntax.
    pub fn render_markdown(&self) -> String {
        match self.tag.as_str() {
            // Inline formatting
            "bold" | "b" | "strong" => {
                format!("**{}**", self.content_as_text())
            }
            "italic" | "i" | "em" => {
                format!("*{}*", self.content_as_text())
            }
            "code" => {
                format!("`{}`", self.content_as_text())
            }
            
            // Headings
            "h1" => format!("# {}", self.content_as_text()),
            "h2" => format!("## {}", self.content_as_text()),
            "h3" => format!("### {}", self.content_as_text()),
            "h4" => format!("#### {}", self.content_as_text()),
            "h5" => format!("##### {}", self.content_as_text()),
            "h6" => format!("###### {}", self.content_as_text()),
            
            // Lists
            "ul" | "list" => {
                self.children
                    .iter()
                    .map(|child| child.render_markdown())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            "li" | "item" => {
                format!("- {}", self.content_as_text())
            }
            
            // Code blocks
            "code_block" => {
                let lang = self.attrs.iter()
                    .find(|(k, _)| k == "language" || k == "lang")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                format!("```{}\n{}\n```", lang, self.content_as_text())
            }
            
            // Paragraphs
            "p" | "paragraph" => {
                let mut parts = vec![];
                if let Some(text) = self.content_as_text_opt() {
                    parts.push(text);
                }
                for child in &self.children {
                    parts.push(child.render_markdown());
                }
                parts.join("")
            }
            
            // Horizontal rule
            "hr" => "---".to_string(),
            
            // Line break
            "br" => "\n".to_string(),
            
            // Key-value pairs (custom semantic tags)
            "kv" => {
                let key = self.attrs.iter()
                    .find(|(k, _)| k == "key")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                let value = self.attrs.iter()
                    .find(|(k, _)| k == "value")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                format!("- **{}:** {}", key, value)
            }
            "kv_code" => {
                let key = self.attrs.iter()
                    .find(|(k, _)| k == "key")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                let value = self.attrs.iter()
                    .find(|(k, _)| k == "value")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                format!("- **{}:** `{}`", key, value)
            }
            
            // For unknown tags or structural tags, just render content and children
            _ => {
                let mut parts = vec![];
                if let Some(text) = self.content_as_text_opt() {
                    parts.push(text);
                }
                for child in &self.children {
                    parts.push(child.render_markdown());
                }
                parts.join(" ")
            }
        }
    }

    /// Helper to get content as text, or empty string if none.
    fn content_as_text(&self) -> String {
        self.content_as_text_opt().unwrap_or_default()
    }

    /// Helper to get content as text option.
    fn content_as_text_opt(&self) -> Option<String> {
        self.content.as_ref().map(|c| match c {
            Content::Text(t) | Content::Raw(t) => t.clone(),
        })
    }

    /// Alias for render_xml() to maintain compatibility.
    pub fn render(&self) -> String {
        self.render_xml()
    }
}

/// Trait for types that can be appended to a Document.
pub trait CanAppendDoc {
    fn append_to_doc(self, doc: Document) -> Document;
}

impl CanAppendDoc for Document {
    fn append_to_doc(self, mut doc: Document) -> Document {
        doc.children.push(self);
        doc
    }
}

impl CanAppendDoc for Option<Document> {
    fn append_to_doc(self, doc: Document) -> Document {
        match self {
            Some(item) => {
                let mut doc = doc;
                doc.children.push(item);
                doc
            }
            None => doc,
        }
    }
}

impl CanAppendDoc for Vec<Document> {
    fn append_to_doc(self, mut doc: Document) -> Document {
        for item in self {
            doc.children.push(item);
        }
        doc
    }
}

impl<const N: usize> CanAppendDoc for [Document; N] {
    fn append_to_doc(self, mut doc: Document) -> Document {
        for item in self {
            doc.children.push(item);
        }
        doc
    }
}

impl Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render_xml())
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    // ============================================================================
    // Basic Document Tests (XML Rendering)
    // ============================================================================

    #[test]
    fn test_document() {
        let doc = Document::new("div");
        let actual = doc.render_xml();
        let expected = "<div></div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_with_attributes() {
        let doc = Document::new("div").attr("class", "test");
        let actual = doc.render_xml();
        let expected = "<div\n  class=\"test\"\n>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_with_children() {
        let doc = Document::new("div")
            .attr("class", "test")
            .append(Document::new("span"));
        let actual = doc.render_xml();
        let expected = "<div\n  class=\"test\"\n>\n<span></span>\n</div>";
        assert_eq!(actual, expected);
    }

    // ============================================================================
    // Markdown Rendering Tests
    // ============================================================================

    #[test]
    fn test_markdown_bold() {
        let doc = Document::new("bold").text("Important");
        assert_eq!(doc.render_xml(), "<bold>Important</bold>");
        assert_eq!(doc.render_markdown(), "**Important**");
    }

    #[test]
    fn test_markdown_italic() {
        let doc = Document::new("italic").text("Emphasis");
        assert_eq!(doc.render_xml(), "<italic>Emphasis</italic>");
        assert_eq!(doc.render_markdown(), "*Emphasis*");
    }

    #[test]
    fn test_markdown_code() {
        let doc = Document::new("code").text("fn main()");
        assert_eq!(doc.render_xml(), "<code>fn main()</code>");
        assert_eq!(doc.render_markdown(), "`fn main()`");
    }

    #[test]
    fn test_markdown_heading() {
        let doc = Document::new("h1").text("Title");
        assert_eq!(doc.render_xml(), "<h1>Title</h1>");
        assert_eq!(doc.render_markdown(), "# Title");
    }

    #[test]
    fn test_markdown_code_block() {
        let doc = Document::new("code_block")
            .attr("language", "rust")
            .cdata("fn main() {}");
        
        assert!(doc.render_xml().contains("<![CDATA[fn main() {}]]>"));
        assert_eq!(doc.render_markdown(), "```rust\nfn main() {}\n```");
    }

    #[test]
    fn test_markdown_list() {
        let doc = Document::new("list").append(vec![
            Document::new("item").text("First"),
            Document::new("item").text("Second"),
        ]);
        
        assert!(doc.render_xml().contains("<list>"));
        assert_eq!(doc.render_markdown(), "- First\n- Second");
    }

    #[test]
    fn test_markdown_kv() {
        let doc = Document::new("kv")
            .attr("key", "Pattern")
            .attr("value", "*.rs");
        
        assert_eq!(doc.render_markdown(), "- **Pattern:** *.rs");
    }

    #[test]
    fn test_markdown_kv_code() {
        let doc = Document::new("kv_code")
            .attr("key", "Path")
            .attr("value", "/home/user");
        
        assert_eq!(doc.render_markdown(), "- **Path:** `/home/user`");
    }

    // ============================================================================
    // XML Rendering Tests - Mirroring Element Tests
    // ============================================================================

    #[test]
    fn test_document_with_multiple_children() {
        let doc = Document::new("div")
            .attr("class", "test")
            .append(vec![Document::new("span"), Document::new("p")]);
        let actual = doc.render_xml();
        let expected = "<div\n  class=\"test\"\n>\n<span></span>\n<p></p>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_with_nested_children() {
        let doc = Document::new("div").attr("class", "test").append([
            Document::new("span").attr("class", "child"),
            Document::new("p").attr("class", "child"),
        ]);
        let actual = doc.render_xml();
        let expected = "<div\n  class=\"test\"\n>\n<span\n  class=\"child\"\n>\n</span>\n<p\n  class=\"child\"\n>\n</p>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_with_text() {
        let doc = Document::new("div")
            .attr("class", "test")
            .text("Hello, world!")
            .append(vec![Document::new("span").attr("class", "child")]);
        let actual = doc.render_xml();
        let expected =
            "<div\n  class=\"test\"\n>Hello, world!\n<span\n  class=\"child\"\n>\n</span>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_with_multiple_classes() {
        let doc = Document::new("div")
            .class("first-class")
            .class("second-class");
        let actual = doc.render_xml();
        let expected = "<div\n  class=\"first-class second-class\"\n>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_with_html_escape() {
        let doc = Document::new("div").text("<script>alert('XSS')</script>");
        let actual = doc.render_xml();
        let expected = "<div>&lt;script&gt;alert('XSS')&lt;/script&gt;</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_with_css_style_classes() {
        let doc = Document::new("div.foo.bar");
        let actual = doc.render_xml();
        let expected = "<div\n  class=\"foo bar\"\n>\n</div>";
        assert_eq!(actual, expected);

        // Test that we can still add more classes
        let doc = Document::new("div.foo.bar").class("extra-class");
        let actual = doc.render_xml();
        let expected = "<div\n  class=\"foo bar extra-class\"\n>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_append_if_some() {
        let doc = Document::new("div").append(Some(Document::new("span")));
        let actual = doc.render_xml();
        let expected = "<div>\n<span></span>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_append_if_none() {
        let doc = Document::new("div").append(None);
        let actual = doc.render_xml();
        let expected = "<div></div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_append_all() {
        let elements = vec![
            Document::new("span").text("First"),
            Document::new("span").text("Second"),
            Document::new("span").text("Third"),
        ];
        let doc = Document::new("div").append(elements);
        let actual = doc.render_xml();
        let expected = "<div>\n<span>First</span>\n<span>Second</span>\n<span>Third</span>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_document_append_all_empty() {
        let elements: Vec<Document> = vec![];
        let doc = Document::new("div").append(elements);
        let actual = doc.render_xml();
        let expected = "<div></div>";
        assert_eq!(actual, expected);
    }

    // ============================================================================
    // Real-world Tool Output Patterns
    // ============================================================================

    #[test]
    fn test_file_read_output() {
        // Pattern: FsRead tool output with file metadata
        let output = Document::new("file")
            .attr("path", "/home/user/test.rs")
            .attr("display_lines", "1-10")
            .attr("total_lines", "50")
            .cdata("fn main() {\n    println!(\"Hello\");\n}");

        let actual = output.render_xml();
        let expected = r#"<file
  path="/home/user/test.rs"
  display_lines="1-10"
  total_lines="50"
><![CDATA[fn main() {
    println!("Hello");
}]]>
</file>"#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_search_results_with_truncation() {
        // Pattern: FsSearch with truncation metadata
        let output = Document::new("search_results")
            .attr("pattern", "TODO")
            .attr("path", "/src")
            .attr("total_lines", "150")
            .attr("display_lines", "1-25")
            .attr("reason", "Results truncated due to exceeding the 25 lines limit")
            .cdata("src/main.rs:10:// TODO: implement\nsrc/lib.rs:5:// TODO: refactor");

        let actual = output.render_xml();
        assert!(actual.contains(r#"pattern="TODO""#));
        assert!(actual.contains(r#"reason="Results truncated"#));
        assert!(actual.contains("<![CDATA["));
    }

    #[test]
    fn test_shell_output_with_streams() {
        // Pattern: Shell command with stdout/stderr
        let output = Document::new("shell_output")
            .attr("command", "cargo test")
            .attr("shell", "/bin/bash")
            .attr("exit_code", "0")
            .append(
                Document::new("stdout")
                    .attr("total_lines", "5")
                    .cdata("running 3 tests\ntest result: ok"),
            )
            .append(Document::new("stderr").attr("total_lines", "0"));

        let actual = output.render_xml();
        assert!(actual.contains(r#"command="cargo test""#));
        assert!(actual.contains("<stdout"));
        assert!(actual.contains("<stderr"));
    }

    #[test]
    fn test_semantic_search_grouped_results() {
        // Pattern: CodebaseSearch with grouped file chunks
        let query_result = Document::new("query_result")
            .attr("query", "authentication logic")
            .attr("use_case", "find auth implementation")
            .attr("results", "3")
            .append(vec![
                Document::new("file")
                    .attr("path", "src/auth.rs")
                    .cdata("fn authenticate(user: &str) -> bool {\n    // auth logic\n}"),
                Document::new("file")
                    .attr("path", "src/login.rs")
                    .cdata("fn login() {\n    authenticate(user);\n}"),
            ]);

        let root = Document::new("sem_search_results").append(query_result);

        let actual = root.render_xml();
        assert!(actual.contains("authentication logic"));
        assert!(actual.contains("src/auth.rs"));
        assert!(actual.contains("src/login.rs"));
    }

    #[test]
    fn test_validation_warning_with_errors() {
        // Pattern: Syntax validation errors
        let warning = Document::new("warning")
            .append(Document::new("message").text("Syntax validation failed"))
            .append(Document::new("file").attr("path", "src/main.rs"))
            .append(Document::new("details").text("The file contains 2 syntax error(s)"))
            .append(vec![
                Document::new("error")
                    .attr("line", "10")
                    .attr("column", "5")
                    .cdata("expected `;`"),
                Document::new("error")
                    .attr("line", "15")
                    .attr("column", "12")
                    .cdata("unexpected token"),
            ])
            .append(Document::new("suggestion").text("Review and fix the syntax issues"));

        let actual = warning.render_xml();
        assert!(actual.contains("Syntax validation failed"));
        assert!(actual.contains(r#"line="10""#));
        assert!(actual.contains("expected `;`"));
    }

    #[test]
    fn test_conditional_attributes() {
        // Pattern: attr_if_some usage
        let with_desc = Document::new("shell_output")
            .attr("command", "ls")
            .attr_if_some("description", Some("List files"));

        let without_desc =
            Document::new("shell_output")
                .attr("command", "ls")
                .attr_if_some("description", None::<String>);

        assert!(with_desc.render_xml().contains(r#"description="List files""#));
        assert!(!without_desc.render_xml().contains("description="));
    }

    #[test]
    fn test_cdata_preserves_special_characters() {
        // Pattern: Code/output that contains XML-like characters
        let code = r#"fn test() {
    let x = "<div>hello</div>";
    if x.contains("&") && x.contains("<") {
        println!("special chars: & < > \" '");
    }
}"#;

        let output = Document::new("code_block")
            .attr("language", "rust")
            .cdata(code);

        let actual = output.render_xml();
        assert!(actual.contains("<![CDATA["));
        assert!(actual.contains(r#"let x = "<div>hello</div>";"#));
        // The raw string literal in code uses escaped quotes, but they appear as regular quotes in output
        assert!(actual.contains(r#"println!("special chars: & < > \" '");"#));
    }
}
