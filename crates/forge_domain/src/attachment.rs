use std::collections::HashSet;

use nom::Parser;
use nom::bytes::complete::{tag, take_until};
use nom::combinator::value;

use crate::Image;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub content: AttachmentContent,
    pub path: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, PartialEq, Eq)]
pub enum AttachmentContent {
    Image(Image),
    FileContent {
        content: String,
        start_line: u64,
        end_line: u64,
        total_lines: u64,
    },
}

impl AttachmentContent {
    pub fn as_image(&self) -> Option<&Image> {
        match self {
            AttachmentContent::Image(image) => Some(image),
            _ => None,
        }
    }

    pub fn contains(&self, text: &str) -> bool {
        match self {
            AttachmentContent::Image(_) => false,
            AttachmentContent::FileContent { content, .. } => content.contains(text),
        }
    }

    pub fn file_content(&self) -> Option<&str> {
        match self {
            AttachmentContent::FileContent { content, .. } => Some(content),
            _ => None,
        }
    }

    pub fn range_info(&self) -> Option<(u64, u64, u64)> {
        match self {
            AttachmentContent::FileContent { start_line, end_line, total_lines, .. } => {
                Some((*start_line, *end_line, *total_lines))
            }
            _ => None,
        }
    }
}

impl Attachment {
    /// Parses a string and extracts all file paths in the format
    /// @[path/to/file]. File paths can contain spaces and are considered to
    /// extend until the closing bracket. If the closing bracket is missing,
    /// consider everything until the end of the string as the path.
    pub fn parse_all<T: ToString>(text: T) -> HashSet<String> {
        let input = text.to_string();
        let mut remaining = input.as_str();
        let mut paths = HashSet::new();
        while !remaining.is_empty() {
            match Self::parse(remaining) {
                Ok((next_remaining, path)) => {
                    paths.insert(path.to_string());
                    remaining = next_remaining;
                }
                Err(_) => {
                    // If parsing fails, we can assume that the remaining string
                    // does not contain any more valid attachments.
                    break;
                }
            }
        }

        paths
    }

    fn parse(input: &str) -> nom::IResult<&str, &str> {
        let (remaining, _) = take_until("@[")(input)?;

        value((), tag("@["))
            .and(take_until("]"))
            .map(|data| data.1)
            .parse(remaining)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_attachment_parse_all_empty() {
        let text = String::from("No attachments here");
        let attachments = Attachment::parse_all(text);
        assert!(attachments.is_empty());
    }

    #[test]
    fn test_attachment_parse_all_simple() {
        let text = String::from("Check this file @[/path/to/file.txt]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let path_found = paths.iter().next().unwrap();
        assert_eq!(path_found, "/path/to/file.txt");
    }

    #[test]
    fn test_attachment_parse_all_with_spaces() {
        let text = String::from("Check this file @[/path/with spaces/file.txt]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let path_found = paths.iter().next().unwrap();
        assert_eq!(path_found, "/path/with spaces/file.txt");
    }

    #[test]
    fn test_attachment_parse_all_multiple() {
        let text = String::from(
            "Check @[/file1.txt] and also @[/path/with spaces/file2.txt] and @[/file3.txt]",
        );
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 3);

        assert!(paths.contains("/file1.txt"));
        assert!(paths.contains("/path/with spaces/file2.txt"));
        assert!(paths.contains("/file3.txt"));
    }

    #[test]
    fn test_attachment_parse_all_at_end() {
        let text = String::from("Check this file @[");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 0);
    }

    #[test]
    fn test_attachment_parse_all_unclosed_bracket() {
        let text = String::from("Check this file @[/path/with spaces/unclosed");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 0);
    }

    #[test]
    fn test_attachment_parse_all_with_multibyte_chars() {
        let text = String::from(
            "Check this file @[🚀/path/with spaces/file.txt🔥] and also @[🌟simple_path]",
        );
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 2);

        assert!(paths.contains("🚀/path/with spaces/file.txt🔥"));
        assert!(paths.contains("🌟simple_path"));
    }
}
