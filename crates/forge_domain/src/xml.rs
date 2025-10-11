/// Extracts content between the specified XML-style tags
///
/// # Arguments
///
/// * `text` - The text to extract content from
/// * `tag_name` - The name of the XML tag (without angle brackets)
///
/// # Returns
///
/// * `Some(&str)` containing the extracted content if tags are found
/// * `None` if the tags are not found
pub fn extract_tag_content<'a>(text: &'a str, tag_name: &str) -> Option<&'a str> {
    let opening_tag = format!("<{tag_name}>",);
    let closing_tag = format!("</{tag_name}>");

    #[allow(clippy::collapsible_if)]
    if let Some(start_idx) = text.find(&opening_tag) {
        if let Some(end_idx) = text.rfind(&closing_tag) {
            let content_start = start_idx + opening_tag.len();
            if content_start < end_idx {
                return Some(text[content_start..end_idx].trim());
            }
        }
    }

    None
}

/// Extracts content from the outermost XML tag found in the text.
/// If no XML tags are found, returns the original text as-is.
///
/// # Arguments
///
/// * `text` - The text to extract content from
///
/// # Returns
///
/// The content inside the outermost XML tag, or the original text if no tags
/// found
pub fn extract_outermost_tag_or_text(text: &str) -> &str {
    // Try to find any opening tag: <tag_name> or <tag_name ...>
    let Ok(tag_pattern) = regex::Regex::new(r"<([a-zA-Z][a-zA-Z0-9_-]*?)(?:\s[^>]*?)?>") else {
        return text;
    };

    // Find first opening tag
    let Some(captures) = tag_pattern.captures(text) else {
        return text;
    };

    let Some(tag_name) = captures.get(1) else {
        return text;
    };

    let Some(opening_match) = captures.get(0) else {
        return text;
    };

    // Find corresponding closing tag
    let closing_tag = format!("</{}>", tag_name.as_str());
    let start_pos = opening_match.end();

    if let Some(end_idx) = text[start_pos..].find(&closing_tag) {
        let content_end = start_pos + end_idx;
        return text[start_pos..content_end].trim();
    }

    // If extraction fails, return original text
    text
}

/// Removes content within XML-style tags that start with the specified prefix
pub fn remove_tag_with_prefix(text: &str, prefix: &str) -> String {
    // First, find all unique tag names that start with the prefix
    let tag_pattern = format!(r"<({prefix}[a-zA-Z0-9_-]*?)(?:\s[^>]*?)?>");
    let mut tag_names = Vec::new();

    if let Ok(regex) = regex::Regex::new(&tag_pattern) {
        for captures in regex.captures_iter(text) {
            if let Some(tag_name) = captures.get(1) {
                // Only add unique tag names to the list
                let tag_name = tag_name.as_str().to_string();
                if !tag_names.contains(&tag_name) {
                    tag_names.push(tag_name);
                }
            }
        }
    }

    // Now remove content for each tag name found
    let mut result = text.to_string();
    for tag_name in tag_names {
        // Create pattern to match complete tag including content
        let pattern = format!(r"<{tag_name}(?:\s[^>]*?)?>[\s\S]*?</{tag_name}>");

        if let Ok(regex) = regex::Regex::new(&pattern) {
            result = regex.replace_all(&result, "").to_string();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_extract_tag_content() {
        let fixture = "Some text <summary>This is the important part</summary> and more text";
        let actual = extract_tag_content(fixture, "summary");
        let expected = Some("This is the important part");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_tag_content_no_tags() {
        let fixture = "Some text without any tags";
        let actual = extract_tag_content(fixture, "summary");
        let expected = None;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_tag_content_with_different_tag() {
        let fixture = "Text with <custom>Custom content</custom> tags";
        let actual = extract_tag_content(fixture, "custom");
        let expected = Some("Custom content");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_tag_content_with_malformed_tags() {
        let fixture = "Text with <opening> but no closing tag";
        let actual = extract_tag_content(fixture, "opening");
        let expected = None;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_tag_names_with_prefix() {
        let fixture = "<forge_tool>Something</forge_tool> <forge_tool_call>Content</forge_tool_call> <other>More</other>";
        let actual = remove_tag_with_prefix(fixture, "forge");
        // Check that both tool tags have been removed, leaving only <other> tag
        assert!(actual.contains("<other>More</other>"));
        assert!(!actual.contains("<forge_tool>"));
        assert!(!actual.contains("<forge_tool_call>"));
    }

    #[test]
    fn test_extract_tag_names_with_prefix_no_matches() {
        let fixture = "<other>Some content</other> <another>Other content</another>";
        let actual = remove_tag_with_prefix(fixture, "forge");
        let expected = "<other>Some content</other> <another>Other content</another>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_tag_names_with_prefix_nested() {
        let fixture = "<parent><forge_tool>Inner</forge_tool><forge_tool_call>Nested</forge_tool_call></parent>";
        let actual = remove_tag_with_prefix(fixture, "forge");
        let expected = "<parent></parent>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_tag_names_with_prefix_duplicates() {
        let fixture =
            "<forge_tool>First</forge_tool><other>Middle</other><forge_tool>Second</forge_tool>";
        let actual = remove_tag_with_prefix(fixture, "forge");
        let expected = "<other>Middle</other>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_tag_names_with_prefix_attributes() {
        let fixture = "<forge_tool id=\"1\">Content</forge_tool> <forge_tool_call class=\"important\">More</forge_tool_call>";
        let actual = remove_tag_with_prefix(fixture, "forge");
        // Check that both tool tags have been removed
        assert!(!actual.contains("<forge_tool"));
        assert!(!actual.contains("<forge_tool_call"));
        assert!(!actual.contains("Content"));
        assert!(!actual.contains("More"));
    }

    #[test]
    fn test_remove_tag_with_prefix() {
        let fixture = "<forge_task>Task details</forge_task> Regular text <forge_analysis>Analysis details</forge_analysis>";
        let actual = remove_tag_with_prefix(fixture, "forge_");
        let expected = " Regular text ";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_remove_tag_with_prefix_no_matching_tags() {
        let fixture = "<other>Content</other> <another>More content</another>";
        let actual = remove_tag_with_prefix(fixture, "forge_");
        let expected = "<other>Content</other> <another>More content</another>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_duplicate_closing_tags() {
        let fixture = "<foo>1<foo>2</foo>3</foo>";
        let actual = extract_tag_content(fixture, "foo").unwrap();
        let expected = "1<foo>2</foo>3";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_outermost_tag_or_text_with_tag() {
        let fixture = "<message>Hello, world!</message>";
        let actual = extract_outermost_tag_or_text(fixture);
        let expected = "Hello, world!";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_outermost_tag_or_text_no_tags() {
        let fixture = "Plain text without tags";
        let actual = extract_outermost_tag_or_text(fixture);
        let expected = "Plain text without tags";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_outermost_tag_or_text_with_nested() {
        let fixture = "<outer>Some <inner>nested</inner> content</outer>";
        let actual = extract_outermost_tag_or_text(fixture);
        let expected = "Some <inner>nested</inner> content";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_outermost_tag_or_text_with_attributes() {
        let fixture = r#"<div class="test" id="main">Content here</div>"#;
        let actual = extract_outermost_tag_or_text(fixture);
        let expected = "Content here";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_outermost_tag_or_text_malformed() {
        let fixture = "<opening>Content without closing";
        let actual = extract_outermost_tag_or_text(fixture);
        let expected = "<opening>Content without closing";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_outermost_tag_or_text_with_text_before_tag() {
        let fixture = "Some prefix <tag>content</tag> some suffix";
        let actual = extract_outermost_tag_or_text(fixture);
        let expected = "content";
        assert_eq!(actual, expected);
    }
}
