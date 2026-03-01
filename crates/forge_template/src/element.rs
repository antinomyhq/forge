use std::fmt::Display;

use convert_case::Casing;

/// The kind of text content stored in an element.
/// Distinguishes between plain text (HTML-escaped) and raw content (CDATA).
#[derive(Debug, Clone, PartialEq)]
pub enum TextKind {
    /// Plain text that should be HTML-escaped when rendering as XML.
    Plain(String),
    /// Raw content that should be wrapped in CDATA for XML rendering
    /// or code blocks for markdown rendering.
    Raw(String),
}

pub enum Element {
    /// A named element with a tag name, attributes, text content, and children.
    Named {
        name: String,
        attr: Vec<(String, String)>,
        children: Vec<Element>,
        text: Option<TextKind>,
    },
    /// An empty element (fragment) without a name.
    /// Can only contain children - no attributes or text content.
    Empty { children: Vec<Element> },
}

impl Element {
    /// Creates a new named element with the given name (can include classes
    /// like "div.container").
    pub fn new(name_with_classes: impl ToString) -> Self {
        let full_name = name_with_classes.to_string();
        let parts: Vec<&str> = full_name.split('.').collect();

        let mut element = Element::Named {
            name: parts[0].to_string(),
            attr: vec![],
            children: vec![],
            text: None,
        };

        // Add classes if there are any
        if parts.len() > 1 {
            let classes = parts[1..].join(" ");
            element = element.attr("class".to_string(), classes);
        }

        element
    }

    /// Creates an empty element (fragment) without a name.
    /// Empty elements can only contain children - no attributes or text
    /// content. Useful for grouping multiple children without a wrapping
    /// tag.
    pub fn empty() -> Self {
        Element::Empty { children: vec![] }
    }

    pub fn span(name: impl ToString) -> Self {
        Element::new("span").text(name)
    }

    pub fn text(mut self, text: impl ToString) -> Self {
        let text_str = text.to_string();
        let encoded = html_escape::encode_text(&text_str);
        match &mut self {
            Element::Named { text: text_field, .. } => {
                *text_field = Some(TextKind::Plain(encoded.to_string()));
            }
            Element::Empty { .. } => {
                // Empty elements cannot have text - return unchanged
            }
        }
        self
    }

    pub fn cdata(mut self, text: impl ToString) -> Self {
        match &mut self {
            Element::Named { text: text_field, .. } => {
                *text_field = Some(TextKind::Raw(text.to_string()));
            }
            Element::Empty { .. } => {
                // Empty elements cannot have text - return unchanged
            }
        }
        self
    }

    pub fn attr(mut self, key: impl ToString, value: impl ToString) -> Self {
        match &mut self {
            Element::Named { attr, .. } => {
                attr.push((key.to_string(), value.to_string()));
            }
            Element::Empty { .. } => {
                // Empty elements cannot have attributes - return unchanged
            }
        }
        self
    }

    pub fn attr_if_some(mut self, key: impl ToString, value: Option<impl ToString>) -> Self {
        if let Some(val) = value {
            self = self.attr(key, val);
        }
        self
    }

    pub fn class(mut self, class_name: impl ToString) -> Self {
        match &mut self {
            Element::Named { attr, .. } => {
                // Check if class attribute already exists
                if let Some(pos) = attr.iter().position(|(key, _)| key == "class") {
                    // Append to existing class
                    let (_, current_class) = &attr[pos];
                    let new_class = format!("{} {}", current_class, class_name.to_string());
                    attr[pos] = ("class".to_string(), new_class);
                } else {
                    // Add new class attribute
                    attr.push(("class".to_string(), class_name.to_string()));
                }
            }
            Element::Empty { .. } => {
                // Empty elements cannot have attributes - return unchanged
            }
        }
        self
    }

    pub fn append(self, item: impl CanAppend) -> Self {
        item.append_to(self)
    }

    /// Renders the element as a markdown string using a heading hierarchy.
    ///
    /// The element name is converted to Title Case and used as a heading at the
    /// given depth (H1 at depth 1, H2 at depth 2, etc., capped at H6).
    /// Attributes are rendered as a bullet list with bold keys and
    /// sentence-case values. Plain text is written as-is, and raw (`cdata`)
    /// content is wrapped in triple-backtick code blocks. Children are
    /// rendered recursively at the next heading level.
    pub fn render_as_markdown(&self) -> String {
        self.render_markdown_internal(1)
    }

    fn render_markdown_internal(&self, level: usize) -> String {
        match self {
            Element::Named { name, attr, children, text } => {
                let mut result = String::new();

                // Heading: name in title case, capped at H6
                let hashes = "#".repeat(level.min(6));
                result.push_str(&format!("{} {}\n\n", hashes, to_title_case(name)));

                // Attributes as bullet points: **key**: Sentence case value (key stays as-is)
                for (key, value) in attr {
                    result.push_str(&format!(
                        "- **{}**: {}\n",
                        to_title_case(key),
                        to_sentence_case(value)
                    ));
                }
                if !attr.is_empty() {
                    result.push('\n');
                }

                // Text content
                if let Some(text) = text {
                    match text {
                        TextKind::Plain(content) => {
                            result.push_str(content);
                            result.push_str("\n\n");
                        }
                        TextKind::Raw(content) => {
                            result.push_str("```\n");
                            result.push_str(content);
                            if !content.ends_with('\n') {
                                result.push('\n');
                            }
                            result.push_str("```\n\n");
                        }
                    }
                }

                // Children at the next heading level
                for child in children {
                    result.push_str(&child.render_markdown_internal(level + 1));
                }

                result
            }
            Element::Empty { children } => {
                let mut result = String::new();
                // Empty elements just render their children directly at the same level
                for child in children {
                    result.push_str(&child.render_markdown_internal(level));
                }
                result
            }
        }
    }

    pub fn render(&self) -> String {
        match self {
            Element::Named { name, attr, children, text } => {
                let mut result = String::new();

                if attr.is_empty() {
                    result.push_str(&format!("<{}>", name));
                } else {
                    result.push_str(&format!("<{}", name));
                    for (key, value) in attr {
                        result.push_str(&format!("\n  {key}=\"{value}\""));
                    }

                    result.push_str("\n>");
                }

                if let Some(text) = text {
                    match text {
                        TextKind::Plain(content) => result.push_str(content),
                        TextKind::Raw(content) => {
                            result.push_str(&format!("<![CDATA[{}]]>", content));
                        }
                    }
                }

                for child in children {
                    result.push('\n');
                    result.push_str(&child.render());
                }

                if children.is_empty() && attr.is_empty() {
                    result.push_str(&format!("</{}>", name));
                } else {
                    result.push_str(&format!("\n</{}>", name));
                }

                result
            }
            Element::Empty { children } => {
                let mut result = String::new();
                // Empty elements render their children directly without wrapping tags
                for child in children {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&child.render());
                }
                result
            }
        }
    }
}

/// Converts a snake_case or kebab-case string to Title Case using convert_case.
fn to_title_case(s: &str) -> String {
    s.to_case(convert_case::Case::Title)
}

/// Converts a string to sentence case: first character uppercased, the rest
/// lowercased, preserving spaces.
fn to_sentence_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
    }
}

pub trait CanAppend {
    fn append_to(self, element: Element) -> Element;
}

impl CanAppend for Element {
    fn append_to(self, mut element: Element) -> Element {
        match &mut element {
            Element::Named { children, .. } => {
                children.push(self);
            }
            Element::Empty { children } => {
                children.push(self);
            }
        }
        element
    }
}

impl<T> CanAppend for T
where
    T: IntoIterator<Item = Element>,
{
    fn append_to(self, mut element: Element) -> Element {
        match &mut element {
            Element::Named { children, .. } => {
                for item in self {
                    children.push(item);
                }
            }
            Element::Empty { children } => {
                for item in self {
                    children.push(item);
                }
            }
        }
        element
    }
}

impl Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render_as_markdown())
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_element() {
        let html = Element::new("div");
        let actual = html.render();
        let expected = "<div></div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_element_with_attributes() {
        let html = Element::new("div").attr("class", "test");
        let actual = html.render();
        let expected = "<div\n  class=\"test\"\n>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_element_with_children() {
        let html = Element::new("div")
            .attr("class", "test")
            .append(Element::new("span"));
        let actual = html.render();
        let expected = "<div\n  class=\"test\"\n>\n<span></span>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_element_with_multiple_children() {
        let html = Element::new("div")
            .attr("class", "test")
            .append([Element::new("span"), Element::new("p")]);
        let actual = html.render();
        let expected = "<div\n  class=\"test\"\n>\n<span></span>\n<p></p>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_element_with_nested_children() {
        let html = Element::new("div").attr("class", "test").append([
            Element::new("span").attr("class", "child"),
            Element::new("p").attr("class", "child"),
        ]);
        let actual = html.render();
        let expected = "<div\n  class=\"test\"\n>\n<span\n  class=\"child\"\n>\n</span>\n<p\n  class=\"child\"\n>\n</p>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_element_with_text() {
        let html = Element::new("div")
            .attr("class", "test")
            .text("Hello, world!")
            .append([Element::new("span").attr("class", "child")]);
        let actual = html.render();
        let expected =
            "<div\n  class=\"test\"\n>Hello, world!\n<span\n  class=\"child\"\n>\n</span>\n</div>";
        assert_eq!(actual, expected);
    }
    #[test]
    fn test_element_with_multiple_classes() {
        let html = Element::new("div")
            .class("first-class")
            .class("second-class");
        let actual = html.render();
        let expected = "<div\n  class=\"first-class second-class\"\n>\n</div>";
        assert_eq!(actual, expected);
    }
    #[test]
    fn test_element_with_html_escape() {
        let html = Element::new("div").text("<script>alert('XSS')</script>");
        let actual = html.render();
        let expected = "<div>&lt;script&gt;alert('XSS')&lt;/script&gt;</div>";
        assert_eq!(actual, expected);
    }
    #[test]
    fn test_element_with_css_style_classes() {
        let html = Element::new("div.foo.bar");
        let actual = html.render();
        let expected = "<div\n  class=\"foo bar\"\n>\n</div>";
        assert_eq!(actual, expected);

        // Test that we can still add more classes
        let html = Element::new("div.foo.bar").class("extra-class");
        let actual = html.render();
        let expected = "<div\n  class=\"foo bar extra-class\"\n>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_if_some() {
        let html = Element::new("div").append(Some(Element::new("span")));
        let actual = html.render();
        let expected = "<div>\n<span></span>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_if_none() {
        let html = Element::new("div").append(None);
        let actual = html.render();
        let expected = "<div></div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_all() {
        let elements = vec![
            Element::new("span").text("First"),
            Element::new("span").text("Second"),
            Element::new("span").text("Third"),
        ];
        let html = Element::new("div").append(elements);
        let actual = html.render();
        let expected = "<div>\n<span>First</span>\n<span>Second</span>\n<span>Third</span>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_all_empty() {
        let elements: Vec<Element> = vec![];
        let html = Element::new("div").append(elements);
        let actual = html.render();
        let expected = "<div></div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_all_with_iterator() {
        let html =
            Element::new("div").append((0..3).map(|i| Element::new("span").text(i.to_string())));
        let actual = html.render();
        let expected = "<div>\n<span>0</span>\n<span>1</span>\n<span>2</span>\n</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_text_kind_plain() {
        let elem = Element::new("div").text("hello");
        // .text() stores HTML content
        if let Element::Named { text: Some(TextKind::Plain(s)), .. } = elem {
            assert_eq!(s, "hello");
        } else {
            panic!("Expected Plain text kind");
        }
    }

    #[test]
    fn test_text_kind_raw() {
        let elem = Element::new("div").cdata("hello");
        // .cdata() stores raw content unescaped
        if let Element::Named { text: Some(TextKind::Raw(s)), .. } = elem {
            assert_eq!(s, "hello");
        } else {
            panic!("Expected Raw text kind");
        }
    }

    #[test]
    fn test_cdata_rendering() {
        let html = Element::new("code").cdata("const x = 1;");
        let actual = html.render();
        let expected = "<code><![CDATA[const x = 1;]]></code>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_as_markdown_simple() {
        let elem = Element::new("shell_output");
        let actual = elem.render_as_markdown();
        let expected = "# Shell Output\n\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_as_markdown_with_attrs() {
        let elem = Element::new("shell_output")
            .attr("command", "cargo build")
            .attr("shell", "zsh");
        let actual = elem.render_as_markdown();
        let expected = "# Shell Output\n\n- **Command**: Cargo build\n- **Shell**: Zsh\n\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_as_markdown_plain_text() {
        let elem = Element::new("message").text("Hello world");
        let actual = elem.render_as_markdown();
        let expected = "# Message\n\nHello world\n\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_as_markdown_cdata() {
        let elem = Element::new("file").cdata("fn main() {}");
        let actual = elem.render_as_markdown();
        let expected = "# File\n\n```\nfn main() {}\n```\n\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_as_markdown_children() {
        let elem = Element::new("shell_output")
            .append(Element::new("stdout").text("Building..."))
            .append(Element::new("stderr").text(""));
        let actual = elem.render_as_markdown();
        let expected = "# Shell Output\n\n## Stdout\n\nBuilding...\n\n## Stderr\n\n\n\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_as_markdown_heading_capped_at_h6() {
        // Build a 7-level deep nesting and verify the deepest renders as H6
        let inner = Element::new("g");
        let elem = Element::new("a").append(Element::new("b").append(Element::new("c").append(
            Element::new("d").append(Element::new("e").append(Element::new("f").append(inner))),
        )));
        let actual = elem.render_as_markdown();
        assert!(actual.contains("###### G"), "Expected H6 for depth 7+");
    }

    #[test]
    fn test_to_title_case() {
        assert_eq!(to_title_case("shell_output"), "Shell Output");
        assert_eq!(to_title_case("forge-tool-call"), "Forge Tool Call");
        assert_eq!(to_title_case("message"), "Message");
    }

    #[test]
    fn test_to_sentence_case() {
        assert_eq!(to_sentence_case("cargo build"), "Cargo build");
        assert_eq!(to_sentence_case("ZSH"), "Zsh");
        assert_eq!(to_sentence_case(""), "");
    }

    #[test]
    fn test_snapshot_markdown_flat_element() {
        let elem = Element::new("message").text("Hello, world!");
        insta::assert_snapshot!(elem.render_as_markdown());
    }

    #[test]
    fn test_snapshot_markdown_attrs_and_cdata() {
        let elem = Element::new("file")
            .attr("path", "src/main.rs")
            .attr("lang", "rust")
            .cdata("fn main() {\n    println!(\"hello\");\n}");
        insta::assert_snapshot!(elem.render_as_markdown());
    }

    #[test]
    fn test_snapshot_markdown_nested_children() {
        let elem = Element::new("shell_output")
            .attr("command", "cargo build")
            .attr("shell", "zsh")
            .append(Element::new("stdout").text("Compiling forge v0.1.0"))
            .append(Element::new("stderr").text("warning: unused variable"));
        insta::assert_snapshot!(elem.render_as_markdown());
    }

    #[test]
    fn test_snapshot_markdown_deeply_nested() {
        let elem = Element::new("tool_result").attr("name", "fs_read").append(
            Element::new("file")
                .attr("path", "README.md")
                .append(Element::new("head").cdata("# Forge\n\nA coding assistant."))
                .append(Element::new("tail").cdata("MIT License")),
        );
        insta::assert_snapshot!(elem.render_as_markdown());
    }

    #[test]
    fn test_empty_element() {
        let elem = Element::empty();
        let actual = elem.render();
        // Empty element with no children renders as empty string
        assert_eq!(actual, "");
    }

    #[test]
    fn test_empty_element_with_children() {
        let elem = Element::empty()
            .append(Element::new("span").text("first"))
            .append(Element::new("span").text("second"));
        let actual = elem.render();
        let expected = "<span>first</span>\n<span>second</span>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_element_ignores_text_attr() {
        // Empty elements should ignore text and attr calls - they remain empty
        let elem = Element::empty()
            .text("this should be ignored")
            .attr("class", "ignored")
            .append(Element::new("div").text("content"));
        let actual = elem.render();
        let expected = "<div>content</div>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_element_markdown() {
        let elem = Element::empty()
            .append(Element::new("message").text("First"))
            .append(Element::new("message").text("Second"));
        let actual = elem.render_as_markdown();
        let expected = "# Message\n\nFirst\n\n# Message\n\nSecond\n\n";
        assert_eq!(actual, expected);
    }
}
