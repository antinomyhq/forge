use std::fmt::Display;

/// Semantic representation of tool output that can be rendered to multiple formats.
///
/// Unlike `Element` which formats strings immediately, `Output` stores semantic parts
/// and defers formatting until `render_xml()` or `render_markdown()` is called.
///
/// # Examples
///
/// ```
/// use forge_template::Output;
///
/// let output = Output::new()
///     .bold("Search Results")
///     .blank_line()
///     .kv_code("Pattern", "*.rs")
///     .kv_code("Path", "/src")
///     .code_block("fn main() {}", Some("rust"));
///
/// // Render as XML for LLM
/// let xml = output.render_xml();
///
/// // Render as Markdown for display
/// let md = output.render_markdown();
/// ```
#[derive(Clone, Debug, Default)]
pub struct Output {
    parts: Vec<OutputPart>,
}

/// Semantic parts that make up an Output.
#[derive(Clone, Debug)]
pub enum OutputPart {
    // Text formatting
    Bold(String),
    Italic(String),
    Code(String),

    // Headings
    Heading { level: u8, text: String },

    // Data display
    KeyValue { key: String, value: String },
    KeyValueCode { key: String, value: String },

    // Content blocks
    CodeBlock { code: String, language: Option<String> },
    Text(String),
    Line(String),
    BlankLine,

    // Lists
    List(Vec<String>),
    ListItem(String),

    // Structural
    Section { title: String, parts: Vec<OutputPart> },

    // Raw element for custom XML tags
    Element {
        name: String,
        attrs: Vec<(String, String)>,
        children: Vec<OutputPart>,
        content: Option<Content>,
    },
}

/// Content types for raw elements.
#[derive(Clone, Debug)]
pub enum Content {
    /// Plain text (will be escaped in XML)
    Text(String),
    /// Raw content (CDATA in XML)
    Raw(String),
}

impl Output {
    /// Creates a new empty Output.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds bold text.
    pub fn bold(mut self, text: impl ToString) -> Self {
        self.parts.push(OutputPart::Bold(text.to_string()));
        self
    }

    /// Adds italic text.
    pub fn italic(mut self, text: impl ToString) -> Self {
        self.parts.push(OutputPart::Italic(text.to_string()));
        self
    }

    /// Adds inline code.
    pub fn code(mut self, text: impl ToString) -> Self {
        self.parts.push(OutputPart::Code(text.to_string()));
        self
    }

    /// Adds a heading at the specified level (1-6).
    pub fn heading(mut self, level: u8, text: impl ToString) -> Self {
        self.parts.push(OutputPart::Heading {
            level: level.min(6).max(1),
            text: text.to_string(),
        });
        self
    }

    /// Adds an H1 heading.
    pub fn h1(self, text: impl ToString) -> Self {
        self.heading(1, text)
    }

    /// Adds an H2 heading.
    pub fn h2(self, text: impl ToString) -> Self {
        self.heading(2, text)
    }

    /// Adds an H3 heading.
    pub fn h3(self, text: impl ToString) -> Self {
        self.heading(3, text)
    }

    /// Adds a key-value pair formatted as "- **key:** value".
    pub fn kv(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.parts.push(OutputPart::KeyValue {
            key: key.to_string(),
            value: value.to_string(),
        });
        self
    }

    /// Adds a key-value pair with the value as inline code.
    pub fn kv_code(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.parts.push(OutputPart::KeyValueCode {
            key: key.to_string(),
            value: value.to_string(),
        });
        self
    }

    /// Adds a code block with optional language.
    pub fn code_block(mut self, code: impl ToString, language: Option<&str>) -> Self {
        self.parts.push(OutputPart::CodeBlock {
            code: code.to_string(),
            language: language.map(String::from),
        });
        self
    }

    /// Adds plain text.
    pub fn text(mut self, text: impl ToString) -> Self {
        self.parts.push(OutputPart::Text(text.to_string()));
        self
    }

    /// Adds a line (text followed by newline).
    pub fn line(mut self, text: impl ToString) -> Self {
        self.parts.push(OutputPart::Line(text.to_string()));
        self
    }

    /// Adds a blank line.
    pub fn blank_line(mut self) -> Self {
        self.parts.push(OutputPart::BlankLine);
        self
    }

    /// Adds a list of items.
    pub fn list(mut self, items: impl IntoIterator<Item = impl ToString>) -> Self {
        let list_items = items.into_iter().map(|item| item.to_string()).collect();
        self.parts.push(OutputPart::List(list_items));
        self
    }

    /// Adds a single list item.
    pub fn list_item(mut self, text: impl ToString) -> Self {
        self.parts.push(OutputPart::ListItem(text.to_string()));
        self
    }

    /// Adds a section with a title and nested content.
    pub fn section(mut self, title: impl ToString, content: Output) -> Self {
        self.parts.push(OutputPart::Section {
            title: title.to_string(),
            parts: content.parts,
        });
        self
    }

    /// Adds a raw OutputPart directly.
    pub fn part(mut self, part: OutputPart) -> Self {
        self.parts.push(part);
        self
    }

    /// Starts building a custom XML element.
    pub fn element(self, name: impl ToString) -> ElementBuilder {
        ElementBuilder {
            output: self,
            name: name.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
            content: None,
        }
    }

    /// Conditionally adds content if the condition is true.
    pub fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if condition {
            f(self)
        } else {
            self
        }
    }

    /// Conditionally adds content if the option is Some.
    pub fn when_some<T>(self, option: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        if let Some(value) = option {
            f(self, value)
        } else {
            self
        }
    }

    /// Renders the output as XML.
    pub fn render_xml(&self) -> String {
        XmlRenderer.render(self)
    }

    /// Renders the output as Markdown.
    pub fn render_markdown(&self) -> String {
        MarkdownRenderer.render(self)
    }

    /// Alias for render_xml() to maintain compatibility.
    pub fn render(&self) -> String {
        self.render_xml()
    }
}

/// Builder for custom XML elements.
pub struct ElementBuilder {
    output: Output,
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<OutputPart>,
    content: Option<Content>,
}

impl ElementBuilder {
    /// Creates a new ElementBuilder for a standalone element (not part of an Output chain).
    pub fn new(name: impl ToString) -> Self {
        Self {
            output: Output::new(),
            name: name.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
            content: None,
        }
    }

    /// Builds the element and returns it as an OutputPart (for use with .child()).
    pub fn build(self) -> OutputPart {
        OutputPart::Element {
            name: self.name,
            attrs: self.attrs,
            children: self.children,
            content: self.content,
        }
    }

    /// Adds an attribute to the element.
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

    /// Adds a CSS class to the element.
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

    /// Sets raw content (CDATA in XML).
    pub fn cdata(mut self, text: impl ToString) -> Self {
        self.content = Some(Content::Raw(text.to_string()));
        self
    }

    /// Adds a child element.
    pub fn child(mut self, child: OutputPart) -> Self {
        self.children.push(child);
        self
    }

    /// Adds multiple children from an iterator.
    pub fn children(mut self, children: impl IntoIterator<Item = OutputPart>) -> Self {
        self.children.extend(children);
        self
    }

    /// Finishes building the element and returns the Output.
    pub fn done(mut self) -> Output {
        self.output.parts.push(OutputPart::Element {
            name: self.name,
            attrs: self.attrs,
            children: self.children,
            content: self.content,
        });
        self.output
    }
}

/// Trait for rendering Output to different formats.
trait OutputRenderer {
    fn render(&self, output: &Output) -> String {
        output
            .parts
            .iter()
            .map(|part| self.render_part(part))
            .collect::<Vec<_>>()
            .join("")
    }

    fn render_part(&self, part: &OutputPart) -> String;
}

/// XML renderer for Output.
struct XmlRenderer;

impl OutputRenderer for XmlRenderer {
    fn render_part(&self, part: &OutputPart) -> String {
        match part {
            OutputPart::Bold(text) => {
                format!("<bold>{}</bold>", html_escape::encode_text(text))
            }
            OutputPart::Italic(text) => {
                format!("<italic>{}</italic>", html_escape::encode_text(text))
            }
            OutputPart::Code(text) => {
                format!("<code>{}</code>", html_escape::encode_text(text))
            }
            OutputPart::Heading { level, text } => {
                format!("<h{level}>{}</h{level}>", html_escape::encode_text(text))
            }
            OutputPart::KeyValue { key, value } => {
                format!(
                    "<kv key=\"{}\" value=\"{}\"></kv>",
                    html_escape::encode_text(key),
                    html_escape::encode_text(value)
                )
            }
            OutputPart::KeyValueCode { key, value } => {
                format!(
                    "<kv_code key=\"{}\" value=\"{}\"></kv_code>",
                    html_escape::encode_text(key),
                    html_escape::encode_text(value)
                )
            }
            OutputPart::CodeBlock { code, language } => {
                let mut result = String::from("<code_block");
                if let Some(lang) = language {
                    result.push_str(&format!(" language=\"{}\"", html_escape::encode_text(lang)));
                }
                result.push_str(&format!("><![CDATA[{}]]></code_block>", code));
                result
            }
            OutputPart::Text(text) => html_escape::encode_text(text).to_string(),
            OutputPart::Line(text) => {
                format!("{}\n", html_escape::encode_text(text))
            }
            OutputPart::BlankLine => "\n".to_string(),
            OutputPart::List(items) => {
                let mut result = String::from("<list>");
                for item in items {
                    result.push_str(&format!("<item>{}</item>", html_escape::encode_text(item)));
                }
                result.push_str("</list>");
                result
            }
            OutputPart::ListItem(text) => {
                format!("<item>{}</item>", html_escape::encode_text(text))
            }
            OutputPart::Section { title, parts } => {
                let mut result = format!("<section title=\"{}\">", html_escape::encode_text(title));
                for part in parts {
                    result.push_str(&self.render_part(part));
                }
                result.push_str("</section>");
                result
            }
            OutputPart::Element {
                name,
                attrs,
                children,
                content,
            } => {
                let mut result = String::new();

                if attrs.is_empty() {
                    result.push_str(&format!("<{}>", name));
                } else {
                    result.push_str(&format!("<{}", name));
                    for (key, value) in attrs {
                        result.push_str(&format!(
                            "\n  {}=\"{}\"",
                            key,
                            html_escape::encode_double_quoted_attribute(value)
                        ));
                    }
                    result.push_str("\n>");
                }

                if let Some(content) = content {
                    match content {
                        Content::Text(text) => {
                            result.push_str(&html_escape::encode_text(text).to_string());
                        }
                        Content::Raw(raw) => {
                            result.push_str(&format!("<![CDATA[{}]]>", raw));
                        }
                    }
                }

                for child in children {
                    result.push('\n');
                    result.push_str(&self.render_part(child));
                }

                if children.is_empty() && attrs.is_empty() {
                    result.push_str(&format!("</{}>", name));
                } else {
                    result.push_str(&format!("\n</{}>", name));
                }

                result
            }
        }
    }
}

/// Markdown renderer for Output.
struct MarkdownRenderer;

impl OutputRenderer for MarkdownRenderer {
    fn render_part(&self, part: &OutputPart) -> String {
        match part {
            OutputPart::Bold(text) => format!("**{}**", text),
            OutputPart::Italic(text) => format!("*{}*", text),
            OutputPart::Code(text) => format!("`{}`", text),
            OutputPart::Heading { level, text } => {
                format!("{} {}", "#".repeat(*level as usize), text)
            }
            OutputPart::KeyValue { key, value } => {
                format!("- **{}:** {}", key, value)
            }
            OutputPart::KeyValueCode { key, value } => {
                format!("- **{}:** `{}`", key, value)
            }
            OutputPart::CodeBlock { code, language } => {
                let lang = language.as_deref().unwrap_or("");
                format!("```{}\n{}\n```", lang, code)
            }
            OutputPart::Text(text) => text.clone(),
            OutputPart::Line(text) => format!("{}\n", text),
            OutputPart::BlankLine => "\n".to_string(),
            OutputPart::List(items) => {
                items
                    .iter()
                    .map(|item| format!("- {}", item))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            OutputPart::ListItem(text) => format!("- {}", text),
            OutputPart::Section { title, parts } => {
                let mut result = format!("## {}\n\n", title);
                for part in parts {
                    result.push_str(&self.render_part(part));
                }
                result
            }
            OutputPart::Element {
                name: _,
                attrs: _,
                children,
                content,
            } => {
                // For elements in markdown, just render content and children
                let mut parts = vec![];
                if let Some(content) = content {
                    match content {
                        Content::Text(t) | Content::Raw(t) => parts.push(t.clone()),
                    }
                }
                for child in children {
                    parts.push(self.render_part(child));
                }
                parts.join(" ")
            }
        }
    }
}

impl Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render_xml())
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    // ============================================================================
    // Basic Output Tests
    // ============================================================================

    #[test]
    fn test_output_bold() {
        let output = Output::new().bold("Important");
        assert_eq!(output.render_xml(), "<bold>Important</bold>");
        assert_eq!(output.render_markdown(), "**Important**");
    }

    #[test]
    fn test_output_italic() {
        let output = Output::new().italic("Emphasis");
        assert_eq!(output.render_xml(), "<italic>Emphasis</italic>");
        assert_eq!(output.render_markdown(), "*Emphasis*");
    }

    #[test]
    fn test_output_code() {
        let output = Output::new().code("fn main()");
        assert_eq!(output.render_xml(), "<code>fn main()</code>");
        assert_eq!(output.render_markdown(), "`fn main()`");
    }

    #[test]
    fn test_output_heading() {
        let output = Output::new().h1("Title");
        assert_eq!(output.render_xml(), "<h1>Title</h1>");
        assert_eq!(output.render_markdown(), "# Title");
    }

    #[test]
    fn test_output_kv() {
        let output = Output::new().kv("Pattern", "*.rs");
        assert_eq!(output.render_xml(), r#"<kv key="Pattern" value="*.rs"></kv>"#);
        assert_eq!(output.render_markdown(), "- **Pattern:** *.rs");
    }

    #[test]
    fn test_output_kv_code() {
        let output = Output::new().kv_code("Path", "/home/user");
        assert_eq!(
            output.render_xml(),
            r#"<kv_code key="Path" value="/home/user"></kv_code>"#
        );
        assert_eq!(output.render_markdown(), "- **Path:** `/home/user`");
    }

    #[test]
    fn test_output_code_block() {
        let output = Output::new().code_block("fn main() {}", Some("rust"));
        assert!(output.render_xml().contains("<![CDATA[fn main() {}]]>"));
        assert!(output.render_xml().contains(r#"language="rust""#));
        assert_eq!(output.render_markdown(), "```rust\nfn main() {}\n```");
    }

    #[test]
    fn test_output_list() {
        let output = Output::new().list(vec!["First", "Second", "Third"]);
        assert!(output.render_xml().contains("<list>"));
        assert!(output.render_xml().contains("<item>First</item>"));
        assert_eq!(output.render_markdown(), "- First\n- Second\n- Third");
    }

    #[test]
    fn test_output_complex() {
        let output = Output::new()
            .bold("Search Results")
            .blank_line()
            .kv_code("Pattern", "*.rs")
            .kv_code("Path", "/src")
            .code_block("fn main() {}", Some("rust"));

        let xml = output.render_xml();
        assert!(xml.contains("<bold>Search Results</bold>"));
        assert!(xml.contains(r#"<kv_code key="Pattern" value="*.rs"></kv_code>"#));

        let md = output.render_markdown();
        assert!(md.contains("**Search Results**"));
        assert!(md.contains("- **Pattern:** `*.rs`"));
        assert!(md.contains("```rust\nfn main() {}\n```"));
    }

    #[test]
    fn test_element_builder() {
        let output = Output::new()
            .element("file")
            .attr("path", "/home/user/test.rs")
            .attr("lines", "10")
            .cdata("fn main() {}")
            .done();

        let xml = output.render_xml();
        assert!(xml.contains(r#"path="/home/user/test.rs""#));
        assert!(xml.contains("<![CDATA[fn main() {}]]>"));
    }

    #[test]
    fn test_html_escape() {
        let output = Output::new().bold("<script>alert('XSS')</script>");
        let xml = output.render_xml();
        assert!(xml.contains("&lt;script&gt;"));
        assert!(!xml.contains("<script>"));
    }

    #[test]
    fn test_conditional_when() {
        let output = Output::new()
            .text("Always shown")
            .when(true, |o| o.text(" - shown"))
            .when(false, |o| o.text(" - hidden"));

        assert!(output.render_xml().contains("Always shown - shown"));
        assert!(!output.render_xml().contains("hidden"));
    }

    #[test]
    fn test_conditional_when_some() {
        let output = Output::new()
            .text("Base")
            .when_some(Some("value"), |o, v| o.text(format!(" - {}", v)))
            .when_some(None::<String>, |o, v| o.text(format!(" - {}", v)));

        assert!(output.render_xml().contains("Base - value"));
    }

    // ============================================================================
    // Real-World Pattern Tests (matching Element tests)
    // ============================================================================

    #[test]
    fn test_file_read_output() {
        let output = Output::new()
            .element("file_contents")
            .attr("path", "/home/user/test.rs")
            .attr("lines", "42")
            .attr("size", "1024")
            .cdata("fn main() {\n    println!(\"Hello\");\n}")
            .done();

        let xml = output.render_xml();
        assert!(xml.contains(r#"path="/home/user/test.rs""#));
        assert!(xml.contains(r#"lines="42""#));
        assert!(xml.contains("<![CDATA[fn main()"));
    }

    #[test]
    fn test_search_results_with_metadata() {
        let output = Output::new()
            .bold("Search Results")
            .kv_code("Pattern", "*.rs")
            .kv_code("Files Found", "15")
            .when(true, |o| o.kv("Truncated", "Yes"))
            .code_block("fn test() {}", Some("rust"));

        let xml = output.render_xml();
        assert!(xml.contains("<bold>Search Results</bold>"));
        assert!(xml.contains(r#"key="Pattern""#));
        assert!(xml.contains(r#"key="Truncated""#));

        let md = output.render_markdown();
        assert!(md.contains("**Search Results**"));
        assert!(md.contains("- **Pattern:** `*.rs`"));
        assert!(md.contains("- **Truncated:** Yes"));
    }

    #[test]
    fn test_shell_output_with_streams() {
        let output = Output::new()
            .element("shell_output")
            .attr("command", "cargo test")
            .attr("exit_code", "0")
            .child(
                ElementBuilder::new("stdout")
                    .cdata("running 10 tests\ntest result: ok")
                    .build(),
            )
            .child(
                ElementBuilder::new("stderr")
                    .cdata("Compiling project v0.1.0")
                    .build(),
            )
            .done();

        let xml = output.render_xml();
        assert!(xml.contains(r#"command="cargo test""#));
        assert!(xml.contains("<stdout>"));
        assert!(xml.contains("<stderr>"));
        assert!(xml.contains("running 10 tests"));
    }

    #[test]
    fn test_validation_errors() {
        let errors = vec![
            ("file.rs", "line 10", "missing semicolon"),
            ("main.rs", "line 5", "unused variable"),
        ];

        let mut output = Output::new().h2("Validation Errors");

        for (file, location, msg) in errors {
            output = output
                .element("error")
                .attr("file", file)
                .attr("location", location)
                .text(msg)
                .done();
        }

        let xml = output.render_xml();
        assert!(xml.contains(r#"file="file.rs""#));
        assert!(xml.contains(r#"location="line 10""#));
        assert!(xml.contains("missing semicolon"));
    }

    #[test]
    fn test_nested_sections() {
        let output = Output::new().section(
            "Main Section",
            Output::new()
                .kv("Key1", "Value1")
                .kv("Key2", "Value2")
                .section(
                    "Subsection",
                    Output::new().text("Nested content").bold("Important"),
                ),
        );

        let xml = output.render_xml();
        assert!(xml.contains("<section"));
        assert!(xml.contains("Main Section"));

        let md = output.render_markdown();
        assert!(md.contains("## Main Section"));
        assert!(md.contains("## Subsection"));
    }

    #[test]
    fn test_optional_attributes() {
        let language: Option<&str> = Some("rust");
        let missing: Option<&str> = None;

        let mut builder = ElementBuilder::new("code_block");
        if let Some(lang) = language {
            builder = builder.attr("language", lang);
        }
        if let Some(m) = missing {
            builder = builder.attr("missing", m);
        }
        let output = Output::new().part(builder.cdata("fn main() {}").build());

        let xml = output.render_xml();
        assert!(xml.contains(r#"language="rust""#));
        assert!(!xml.contains("missing="));
    }

    #[test]
    fn test_cdata_with_xml_content() {
        let code = r#"let x = "<div class=\"test\">content</div>";"#;
        let output = Output::new().code_block(code, Some("rust"));

        let xml = output.render_xml();
        assert!(xml.contains("<![CDATA["));
        assert!(xml.contains(code));
        assert!(!xml.contains("&lt;div&gt;")); // CDATA should not escape
    }

    #[test]
    fn test_multiple_items_builder() {
        let files = vec!["main.rs", "lib.rs", "test.rs"];

        let mut output = Output::new().h2("Files");
        for file in files {
            output = output
                .element("file")
                .attr("name", file)
                .attr("type", "rust")
                .done();
        }

        let xml = output.render_xml();
        assert!(xml.contains(r#"name="main.rs""#));
        assert!(xml.contains(r#"name="lib.rs""#));
        assert!(xml.contains(r#"type="rust""#));
    }

    #[test]
    fn test_mixed_content() {
        let output = Output::new()
            .text("Regular text ")
            .bold("bold text")
            .text(" more text ")
            .code("code")
            .blank_line()
            .list(vec!["item1", "item2"]);

        let md = output.render_markdown();
        assert!(md.contains("Regular text **bold text** more text `code`"));
        assert!(md.contains("- item1"));
    }

    #[test]
    fn test_empty_output() {
        let output = Output::new();
        assert_eq!(output.render_xml(), "");
        assert_eq!(output.render_markdown(), "");
    }

    #[test]
    fn test_chained_builders() {
        let output = Output::new()
            .element("outer")
            .attr("id", "1")
            .child(ElementBuilder::new("inner").attr("id", "2").text("content").build())
            .done()
            .element("sibling")
            .attr("id", "3")
            .done();

        let xml = output.render_xml();
        assert!(xml.contains(r#"<outer"#));
        assert!(xml.contains(r#"<inner"#));
        assert!(xml.contains(r#"<sibling"#));
    }

    #[test]
    fn test_large_collection() {
        let mut output = Output::new().h2("Large Collection");
        for i in 0..50 {
            output = output.kv(&format!("Item{}", i), &format!("Value{}", i));
        }

        let xml = output.render_xml();
        assert!(xml.contains(r#"key="Item0""#));
        assert!(xml.contains(r#"key="Item49""#));

        let md = output.render_markdown();
        assert!(md.contains("- **Item0:** Value0"));
        assert!(md.contains("- **Item49:** Value49"));
    }

    #[test]
    fn test_special_characters_in_attributes() {
        let output = Output::new()
            .element("test")
            .attr("quote", r#"He said "hello""#)
            .attr("ampersand", "Tom & Jerry")
            .attr("less", "x < 5")
            .done();

        let xml = output.render_xml();
        assert!(xml.contains("&quot;"));
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&lt;"));
    }
}
