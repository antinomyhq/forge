use nom::Parser;
use nom::bytes::complete::tag;

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
    pub fn parse_all<T: ToString>(text: T) -> Vec<FileTag> {
        let input = text.to_string();
        let mut remaining = input.as_str();
        let mut tags = Vec::new();

        while !remaining.is_empty() {
            // Find the next "@[" pattern
            if let Some(start_pos) = remaining.find("@[") {
                // Move to the position where "@[" starts
                remaining = &remaining[start_pos..];
                match FileTag::parse(remaining) {
                    Ok((next_remaining, file_tag)) => {
                        tags.push(file_tag);
                        remaining = next_remaining;
                    }
                    Err(_e) => {
                        // Skip the "@[" since we couldn't parse it
                        remaining = &remaining[2..];
                    }
                }
            } else {
                // No more "@[" patterns found
                break;
            }
        }

        let mut seen = std::collections::HashSet::new();
        tags.retain(|tag| seen.insert((tag.path.clone(), tag.loc.clone(), tag.symbol.clone())));

        tags
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Location {
    pub start: Option<u64>,
    pub end: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileTag {
    pub path: String,
    pub loc: Option<Location>,
    pub symbol: Option<String>,
}

impl FileTag {
    pub fn parse(input: &str) -> nom::IResult<&str, FileTag> {
        use nom::bytes::complete::take_while1;
        use nom::character::complete::{char, digit1};
        use nom::combinator::{map_res, opt};
        use nom::sequence::{delimited, preceded};

        let parse_u64 = || map_res(digit1, str::parse::<u64>);
        let parse_symbol = preceded(char('#'), take_while1(|c: char| c != ']'));

        let parse_location_full = (
            preceded(char(':'), parse_u64()),
            preceded(char(':'), parse_u64()),
        );
        let parse_location_start_only = preceded(char(':'), parse_u64());

        let parse_location = nom::branch::alt((
            nom::combinator::map(parse_location_full, |(start, end)| (Some(start), Some(end))),
            nom::combinator::map(parse_location_start_only, |start| (Some(start), None)),
        ));

        let parse_path = take_while1(|c: char| c != ':' && c != '#' && c != ']');
        let mut parser = delimited(
            tag("@["),
            (parse_path, opt(parse_location), opt(parse_symbol)),
            char(']'),
        );

        let (remaining, (path, location, symbol)) = parser.parse(input)?;
        let loc = location.map(|(start, end)| Location { start, end });
        Ok((
            remaining,
            FileTag {
                path: path.to_string(),
                loc,
                symbol: symbol.map(|s| s.to_string()),
            },
        ))
    }
}

impl AsRef<std::path::Path> for FileTag {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.path)
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
        assert_eq!(path_found.path, "/path/to/file.txt");
    }

    #[test]
    fn test_attachment_parse_all_with_spaces() {
        let text = String::from("Check this file @[/path/with spaces/file.txt]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let path_found = paths.iter().next().unwrap();
        assert_eq!(path_found.path, "/path/with spaces/file.txt");
    }

    #[test]
    fn test_attachment_parse_all_multiple() {
        let text = String::from(
            "Check @[/file1.txt] and also @[/path/with spaces/file2.txt] and @[/file3.txt]",
        );
        let paths = Attachment::parse_all(text);
        let paths = paths
            .iter()
            .map(|tag| tag.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths.len(), 3);

        assert!(paths.contains(&"/file1.txt"));
        assert!(paths.contains(&"/path/with spaces/file2.txt"));
        assert!(paths.contains(&"/file3.txt"));
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
        let paths = paths
            .iter()
            .map(|tag| tag.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths.len(), 2);

        assert!(paths.contains(&"🚀/path/with spaces/file.txt🔥"));
        assert!(paths.contains(&"🌟simple_path"));
    }

    #[test]
    fn test_attachment_parse_with_location() {
        let text = String::from("Check line @[/path/to/file.txt:10:20]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let expected = FileTag {
            path: "/path/to/file.txt".to_string(),
            loc: Some(Location { start: Some(10), end: Some(20) }),
            symbol: None,
        };
        let actual = paths.iter().next().unwrap();
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_attachment_parse_with_symbol() {
        let text = String::from("Check function @[/path/to/file.rs#my_function]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let expected = FileTag {
            path: "/path/to/file.rs".to_string(),
            loc: None,
            symbol: Some("my_function".to_string()),
        };
        let actual = paths.iter().next().unwrap();
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_attachment_parse_with_location_and_symbol() {
        let text = String::from("Check @[/src/main.rs:5:15#main_function]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let expected = FileTag {
            path: "/src/main.rs".to_string(),
            loc: Some(Location { start: Some(5), end: Some(15) }),
            symbol: Some("main_function".to_string()),
        };
        let actual = paths.iter().next().unwrap();
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_attachment_parse_multiple_with_mixed_features() {
        let text = String::from(
            "Check @[/file1.txt] and @[/file2.rs:10:20] and @[/file3.py#function] and @[/file4.js:1:5#init]",
        );
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 4);

        let expected = vec![
            FileTag { path: "/file1.txt".to_string(), loc: None, symbol: None },
            FileTag {
                path: "/file2.rs".to_string(),
                loc: Some(Location { start: Some(10), end: Some(20) }),
                symbol: None,
            },
            FileTag {
                path: "/file3.py".to_string(),
                loc: None,
                symbol: Some("function".to_string()),
            },
            FileTag {
                path: "/file4.js".to_string(),
                loc: Some(Location { start: Some(1), end: Some(5) }),
                symbol: Some("init".to_string()),
            },
        ];

        for expected_tag in expected {
            assert!(paths.contains(&expected_tag));
        }
    }

    #[test]
    fn test_attachment_parse_symbol_with_special_chars() {
        let text = String::from("Check @[/file.rs#function_with_underscore_123]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let expected = FileTag {
            path: "/file.rs".to_string(),
            loc: None,
            symbol: Some("function_with_underscore_123".to_string()),
        };
        let actual = paths.iter().next().unwrap();
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_attachment_parse_location_edge_cases() {
        let text = String::from("Check @[/file.txt:0:999999]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let expected = FileTag {
            path: "/file.txt".to_string(),
            loc: Some(Location { start: Some(0), end: Some(999999) }),
            symbol: None,
        };
        let actual = paths.iter().next().unwrap();
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_attachment_parse_location_with_start() {
        let text = String::from("Check @[/file.txt:12#main()]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let expected = FileTag {
            path: "/file.txt".to_string(),
            loc: Some(Location { start: Some(12), end: None }),
            symbol: Some("main()".to_string()),
        };
        let actual = paths.iter().next().unwrap();
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_attachment_parse_location_duplicate_entries() {
        let text = String::from("Check @[/file.txt:12#main()] and @[/file.txt:12#main()]");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let expected = FileTag {
            path: "/file.txt".to_string(),
            loc: Some(Location { start: Some(12), end: None }),
            symbol: Some("main()".to_string()),
        };
        let actual = paths.iter().next().unwrap();
        assert_eq!(actual, &expected);
    }
}
