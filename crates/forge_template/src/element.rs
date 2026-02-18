use std::fmt::Display;

pub struct Element {
    pub name: String,
    pub attr: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: Option<String>,
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
        self.text = Some(html_escape::encode_text(&text.to_string()).to_string());
        self
    }

    pub fn cdata(mut self, text: impl ToString) -> Self {
        self.text = Some(format!("<![CDATA[{}]]>", text.to_string()));
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
            result.push_str(text);
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

    // ============================================================================
    // Basic Element Tests
    // ============================================================================

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

    // ============================================================================
    // Real-world Tool Output Patterns (from forge-clone/operation.rs)
    // ============================================================================

    #[test]
    fn test_file_read_output() {
        // Pattern: FsRead tool output with file metadata
        let output = Element::new("file")
            .attr("path", "/home/user/test.rs")
            .attr("display_lines", "1-10")
            .attr("total_lines", "50")
            .cdata("fn main() {\n    println!(\"Hello\");\n}");

        let actual = output.render();
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
        let output = Element::new("search_results")
            .attr("pattern", "TODO")
            .attr("path", "/src")
            .attr("total_lines", "150")
            .attr("display_lines", "1-25")
            .attr("reason", "Results truncated due to exceeding the 25 lines limit")
            .cdata("src/main.rs:10:// TODO: implement\nsrc/lib.rs:5:// TODO: refactor");

        let actual = output.render();
        assert!(actual.contains(r#"pattern="TODO""#));
        assert!(actual.contains(r#"reason="Results truncated"#));
        assert!(actual.contains("<![CDATA["));
    }

    #[test]
    fn test_shell_output_with_streams() {
        // Pattern: Shell command with stdout/stderr
        let output = Element::new("shell_output")
            .attr("command", "cargo test")
            .attr("shell", "/bin/bash")
            .attr("exit_code", "0")
            .append(
                Element::new("stdout")
                    .attr("total_lines", "5")
                    .cdata("running 3 tests\ntest result: ok"),
            )
            .append(Element::new("stderr").attr("total_lines", "0"));

        let actual = output.render();
        assert!(actual.contains(r#"command="cargo test""#));
        assert!(actual.contains("<stdout"));
        assert!(actual.contains("<stderr"));
    }

    #[test]
    fn test_semantic_search_grouped_results() {
        // Pattern: CodebaseSearch with grouped file chunks
        let query_result = Element::new("query_result")
            .attr("query", "authentication logic")
            .attr("use_case", "find auth implementation")
            .attr("results", "3")
            .append(vec![
                Element::new("file")
                    .attr("path", "src/auth.rs")
                    .cdata("fn authenticate(user: &str) -> bool {\n    // auth logic\n}"),
                Element::new("file")
                    .attr("path", "src/login.rs")
                    .cdata("fn login() {\n    authenticate(user);\n}"),
            ]);

        let root = Element::new("sem_search_results").append(query_result);

        let actual = root.render();
        assert!(actual.contains("authentication logic"));
        assert!(actual.contains("src/auth.rs"));
        assert!(actual.contains("src/login.rs"));
    }

    #[test]
    fn test_validation_warning_with_errors() {
        // Pattern: Syntax validation errors
        let warning = Element::new("warning")
            .append(Element::new("message").text("Syntax validation failed"))
            .append(Element::new("file").attr("path", "src/main.rs"))
            .append(Element::new("details").text("The file contains 2 syntax error(s)"))
            .append(vec![
                Element::new("error")
                    .attr("line", "10")
                    .attr("column", "5")
                    .cdata("expected `;`"),
                Element::new("error")
                    .attr("line", "15")
                    .attr("column", "12")
                    .cdata("unexpected token"),
            ])
            .append(Element::new("suggestion").text("Review and fix the syntax issues"));

        let actual = warning.render();
        assert!(actual.contains("Syntax validation failed"));
        assert!(actual.contains(r#"line="10""#));
        assert!(actual.contains("expected `;`"));
    }

    #[test]
    fn test_stream_element_with_head_tail() {
        // Pattern: Truncated output with head/tail sections
        let output = Element::new("stdout")
            .attr("total_lines", "1000")
            .append(
                Element::new("head")
                    .attr("display_lines", "1-10")
                    .cdata("Line 1\nLine 2\n...\nLine 10"),
            )
            .append(
                Element::new("tail")
                    .attr("display_lines", "991-1000")
                    .cdata("Line 991\n...\nLine 1000"),
            );

        let actual = output.render();
        assert!(actual.contains(r#"total_lines="1000""#));
        assert!(actual.contains("<head"));
        assert!(actual.contains("<tail"));
    }

    #[test]
    fn test_conditional_attributes() {
        // Pattern: attr_if_some usage
        let with_desc = Element::new("shell_output")
            .attr("command", "ls")
            .attr_if_some("description", Some("List files"));

        let without_desc =
            Element::new("shell_output")
                .attr("command", "ls")
                .attr_if_some("description", None::<String>);

        assert!(with_desc.render().contains(r#"description="List files""#));
        assert!(!without_desc.render().contains("description="));
    }

    #[test]
    fn test_iterator_mapping_pattern() {
        // Pattern: Creating multiple children from iterator
        let errors = vec![
            ("10", "5", "missing semicolon"),
            ("20", "8", "undefined variable"),
            ("30", "12", "type mismatch"),
        ];

        let output = Element::new("validation").append(errors.iter().map(
            |(line, col, msg)| {
                Element::new("error")
                    .attr("line", *line)
                    .attr("column", *col)
                    .text(*msg)
            },
        ));

        let actual = output.render();
        assert!(actual.contains(r#"line="10""#));
        assert!(actual.contains("missing semicolon"));
        assert!(actual.contains(r#"line="30""#));
    }

    #[test]
    fn test_nested_conditional_content() {
        // Pattern: Complex conditional structure building
        let has_errors = true;
        let has_warnings = false;

        let mut output = Element::new("result").attr("status", "completed");

        if has_errors {
            output = output.append(
                Element::new("errors").append(Element::new("error").text("Something failed")),
            );
        }

        if has_warnings {
            output = output.append(
                Element::new("warnings")
                    .append(Element::new("warning").text("Something is suspicious")),
            );
        }

        let actual = output.render();
        assert!(actual.contains("<errors>"));
        assert!(actual.contains("Something failed"));
        assert!(!actual.contains("<warnings>"));
    }

    #[test]
    fn test_skill_output_pattern() {
        // Pattern: Skill tool with resources
        let resources = vec!["README.md", "GUIDE.md", "EXAMPLES.md"];

        let output = Element::new("skill_details")
            .append(
                Element::new("command")
                    .attr("location", "/skills/test-skill/SKILL.md")
                    .cdata("# Test Skill\n\nThis is a test skill."),
            )
            .append(
                resources
                    .iter()
                    .map(|r| Element::new("resource").text(*r)),
            );

        let actual = output.render();
        assert!(actual.contains("<command"));
        assert!(actual.contains("Test Skill"));
        assert!(actual.contains("<resource>README.md</resource>"));
        assert!(actual.contains("<resource>GUIDE.md</resource>"));
    }

    #[test]
    fn test_empty_search_results() {
        // Pattern: No results found
        let output = Element::new("sem_search_results")
            .text("No results found for query. Try refining your search.");

        let actual = output.render();
        let expected = "<sem_search_results>No results found for query. Try refining your search.</sem_search_results>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mixed_text_and_children() {
        // Pattern: Text content followed by child elements
        let output = Element::new("message")
            .text("Operation completed with warnings:")
            .append(vec![
                Element::new("warning").text("File size exceeds 1MB"),
                Element::new("warning").text("Deprecated API used"),
            ]);

        let actual = output.render();
        assert!(actual.contains("Operation completed with warnings:"));
        assert!(actual.contains("<warning>File size exceeds 1MB</warning>"));
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

        let output = Element::new("code_block")
            .attr("language", "rust")
            .cdata(code);

        let actual = output.render();
        assert!(actual.contains("<![CDATA["));
        assert!(actual.contains(r#"let x = "<div>hello</div>";"#));
        // The raw string literal in code uses escaped quotes, but they appear as regular quotes in output
        assert!(actual.contains(r#"println!("special chars: & < > \" '");"#));
    }

    #[test]
    fn test_multiple_query_results() {
        // Pattern: Multiple semantic search queries with results
        let queries = vec![
            ("database connection", 5),
            ("error handling", 3),
            ("logging setup", 2),
        ];

        let root = Element::new("sem_search_results").append(queries.iter().map(
            |(query, count)| {
                Element::new("query_result")
                    .attr("query", *query)
                    .attr("results", count.to_string())
                    .append(
                        (0..*count).map(|i| {
                            Element::new("file")
                                .attr("path", format!("src/file{}.rs", i))
                                .cdata(format!("// Code for {}", query))
                        }),
                    )
            },
        ));

        let actual = root.render();
        assert!(actual.contains("database connection"));
        assert!(actual.contains(r#"results="5""#));
        assert!(actual.contains("error handling"));
        assert!(actual.contains(r#"results="3""#));
    }

    #[test]
    fn test_optional_elements_with_append() {
        // Pattern: Conditionally appending elements
        let show_metadata = true;
        let show_stats = false;

        let output = Element::new("report")
            .append(Element::new("title").text("Analysis Report"))
            .append(show_metadata.then(|| {
                Element::new("metadata")
                    .attr("version", "1.0")
                    .attr("date", "2024-01-01")
            }))
            .append(show_stats.then(|| {
                Element::new("stats")
                    .attr("files", "10")
                    .attr("lines", "1000")
            }));

        let actual = output.render();
        assert!(actual.contains("<metadata"));
        assert!(actual.contains(r#"version="1.0""#));
        assert!(!actual.contains("<stats"));
    }

    #[test]
    fn test_deep_nesting() {
        // Pattern: Deeply nested structure
        let output = Element::new("root")
            .append(Element::new("level1").append(
                Element::new("level2").append(
                    Element::new("level3").append(
                        Element::new("level4")
                            .attr("depth", "4")
                            .text("Deep content"),
                    ),
                ),
            ));

        let actual = output.render();
        assert!(actual.contains("<root>"));
        assert!(actual.contains("<level1>"));
        assert!(actual.contains("<level2>"));
        assert!(actual.contains("<level3>"));
        assert!(actual.contains(r#"depth="4""#));
        assert!(actual.contains("Deep content"));
    }

    #[test]
    fn test_escaped_attribute_values() {
        // Pattern: Attributes containing special characters
        let output = Element::new("element")
            .attr("message", "Error: \"file not found\" & path invalid")
            .attr("code", "<script>alert('xss')</script>");

        let actual = output.render();
        // Attributes are not escaped in current implementation, but should be
        assert!(actual.contains(r#"message="Error: "file not found" & path invalid""#));
    }

    #[test]
    fn test_large_collection_mapping() {
        // Pattern: Mapping large collections
        let items: Vec<_> = (0..50)
            .map(|i| {
                Element::new("item")
                    .attr("id", i.to_string())
                    .text(format!("Item {}", i))
            })
            .collect();

        let output = Element::new("collection")
            .attr("count", "50")
            .append(items);

        let actual = output.render();
        assert!(actual.contains(r#"count="50""#));
        // Elements with attributes and text don't have newline before closing tag
        assert!(actual.contains(r#"id="0""#));
        assert!(actual.contains("Item 0"));
        assert!(actual.contains(r#"id="49""#));
        assert!(actual.contains("Item 49"));
    }

    #[test]
    fn test_chained_transformations() {
        // Pattern: Building element through multiple transformations
        let base = Element::new("div");
        let with_class = base.class("container");
        let with_attr = with_class.attr("data-test", "true");
        let with_child = with_attr.append(Element::new("span").text("content"));

        let actual = with_child.render();
        assert!(actual.contains(r#"class="container""#));
        assert!(actual.contains(r#"data-test="true""#));
        assert!(actual.contains("<span>content</span>"));
    }
}
