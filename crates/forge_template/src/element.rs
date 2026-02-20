use std::fmt::Display;

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

pub struct Element {
    pub name: String,
    pub attr: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: Option<TextKind>,
}

impl Element {
    pub fn new(name_with_classes: impl ToString) -> Self {
        let full_name = name_with_classes.to_string();
        let parts: Vec<&str> = full_name.split('.').collect();

        let mut element = Element {
            name: parts[0].to_string(),
            attr: vec![],
            children: vec![],
            text: None,
        };

        // Add classes if there are any
        if parts.len() > 1 {
            let classes = parts[1..].join(" ");
            element.attr.push(("class".to_string(), classes));
        }

        element
    }

    pub fn span(name: impl ToString) -> Self {
        Element::new("span").text(name)
    }

    pub fn text(mut self, text: impl ToString) -> Self {
        let text_str = text.to_string();
        let encoded = html_escape::encode_text(&text_str);
        self.text = Some(TextKind::Plain(encoded.to_string()));
        self
    }

    pub fn cdata(mut self, text: impl ToString) -> Self {
        self.text = Some(TextKind::Raw(text.to_string()));
        self
    }

    pub fn attr(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.attr.push((key.to_string(), value.to_string()));
        self
    }
    pub fn attr_if_some(mut self, key: impl ToString, value: Option<impl ToString>) -> Self {
        if let Some(val) = value {
            self.attr.push((key.to_string(), val.to_string()));
        }
        self
    }
    pub fn class(mut self, class_name: impl ToString) -> Self {
        // Check if class attribute already exists
        if let Some(pos) = self.attr.iter().position(|(key, _)| key == "class") {
            // Append to existing class
            let (_, current_class) = &self.attr[pos];
            let new_class = format!("{} {}", current_class, class_name.to_string());
            self.attr[pos] = ("class".to_string(), new_class);
        } else {
            // Add new class attribute
            self.attr
                .push(("class".to_string(), class_name.to_string()));
        }
        self
    }

    pub fn append(self, item: impl CanAppend) -> Self {
        item.append_to(self)
    }

    pub fn render(&self) -> String {
        let mut result = String::new();

        if self.attr.is_empty() {
            result.push_str(&format!("<{}>", self.name));
        } else {
            result.push_str(&format!("<{}", self.name));
            for (key, value) in &self.attr {
                result.push_str(&format!("\n  {key}=\"{value}\""));
            }

            result.push_str("\n>");
        }

        if let Some(ref text) = self.text {
            match text {
                TextKind::Plain(content) => result.push_str(content),
                TextKind::Raw(content) => {
                    result.push_str(&format!("<![CDATA[{}]]>", content));
                }
            }
        }

        for child in &self.children {
            result.push('\n');
            result.push_str(&child.render());
        }

        if self.children.is_empty() && self.attr.is_empty() {
            result.push_str(&format!("</{}>", self.name));
        } else {
            result.push_str(&format!("\n</{}>", self.name));
        }

        result
    }
}

pub trait CanAppend {
    fn append_to(self, element: Element) -> Element;
}

impl CanAppend for Element {
    fn append_to(self, mut element: Element) -> Element {
        element.children.push(self);
        element
    }
}

impl<T> CanAppend for T
where
    T: IntoIterator<Item = Element>,
{
    fn append_to(self, mut element: Element) -> Element {
        for item in self {
            element.children.push(item);
        }
        element
    }
}

impl Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
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
        let elem = Element::new("div").text("<script>");
        assert_eq!(elem.text, Some(TextKind::Plain("&lt;script&gt;".to_string())));
    }

    #[test]
    fn test_text_kind_raw() {
        let elem = Element::new("div").cdata("<script>");
        assert_eq!(elem.text, Some(TextKind::Raw("<script>".to_string())));
    }

    #[test]
    fn test_cdata_rendering() {
        let html = Element::new("code").cdata("const x = 1;");
        let actual = html.render();
        let expected = "<code><![CDATA[const x = 1;]]></code>";
        assert_eq!(actual, expected);
    }
}
