use std::collections::HashSet;

use nom::bytes::complete::{tag, take_until, take_while1};
use nom::sequence::preceded;
use nom::Parser;
use url::form_urlencoded;

use crate::Image;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub content: AttachmentContent,
    pub path: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, PartialEq, Eq)]
pub enum AttachmentContent {
    Image(Image),
    FileContent(String),
}

impl Attachment {
    /// Parses a string and extracts all file paths in the format @path/to/file.
    /// File paths are considered to extend until whitespace is encountered.
    /// If the path is URL encoded (contains % characters), it will be decoded.
    pub fn parse_all<T: ToString>(text: T) -> HashSet<String> {
        let input = text.to_string();
        let mut remaining = input.as_str();
        let mut paths = HashSet::new();
        while !remaining.is_empty() {
            match Self::parse(remaining) {
                Ok((next_remaining, path)) => {
                    // Decode URL-encoded path if necessary
                    let decoded_path = if path.contains('%') {
                        form_urlencoded::parse(path.as_bytes())
                            .map(|(k, v)| format!("{k}{v}"))
                            .collect::<String>()
                    } else {
                        path.to_string()
                    };
                    paths.insert(decoded_path);
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
        // Find @ symbol
        let (remaining, _) = take_until("@")(input)?;

        // Parse the path after @ until whitespace or another special char
        preceded(
            tag("@"),
            take_while1(|c: char| !c.is_whitespace() && c != '@'),
        )
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
        let text = String::from("Check this file @/path/to/file.txt");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let path_found = paths.iter().next().unwrap();
        assert_eq!(path_found, "/path/to/file.txt");
    }

    #[test]
    fn test_attachment_parse_all_with_spaces() {
        let text = String::from("Check this file @/path/with%20spaces/file.txt");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let path_found = paths.iter().next().unwrap();
        assert_eq!(path_found, "/path/with spaces/file.txt");
    }

    #[test]
    fn test_attachment_parse_all_multiple() {
        let text = String::from(
            "Check @/file1.txt and also @/path/with%20spaces/file2.txt and @/file3.txt",
        );
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 3);

        assert!(paths.contains("/file1.txt"));
        assert!(paths.contains("/path/with spaces/file2.txt"));
        assert!(paths.contains("/file3.txt"));
    }

    #[test]
    fn test_attachment_parse_all_at_end() {
        let text = String::from("Check this file @");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 0);
    }

    #[test]
    fn test_attachment_parse_all_with_multibyte_chars() {
        let text = String::from(
            "Check this file @🚀/path/with%20spaces/file.txt🔥 and also @🌟simple_path",
        );
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 2);

        assert!(paths.contains("🚀/path/with spaces/file.txt🔥"));
        assert!(paths.contains("🌟simple_path"));
    }
}
