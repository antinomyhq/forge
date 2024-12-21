use nom::branch::alt;
use nom::bytes::complete::take_while1;
use nom::character::complete::{char, space0, space1};
use nom::combinator::{map, opt, recognize};
use nom::multi::many0;
use nom::sequence::{pair, preceded};
use nom::IResult;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Prompt {
    pub tokens: Vec<Token>,
}

impl Prompt {
    pub fn message(&self) -> String {
        self.tokens
            .iter()
            .map(|t| match t {
                Token::Literal(s) => s.clone(),
                Token::File(s) => format!("@{}", s),
            })
            .collect::<Vec<String>>()
            .join(" ")
    }

    pub fn files(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.tokens
            .iter()
            .filter_map(|t| match t {
                Token::File(s) => {
                    if seen.insert(s.clone()) {
                        Some(s.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub enum Token {
    Literal(String),
    File(String),
}

impl Prompt {
    // TODO: make `parse` pub(crate)
    pub fn parse(message: String) -> Result<Prompt, String> {
        let tokens = match Self::parse_tokens(&message) {
            Ok((_, tokens)) => tokens,
            Err(_) => vec![Token::Literal(message)], // Fallback for unparsable input
        };

        Ok(Prompt { tokens })
    }

    fn parse_tokens(input: &str) -> IResult<&str, Vec<Token>> {
        many0(alt((
            Self::parse_file_path,
            map(Self::parse_word, Token::Literal),
        )))(input)
    }

    fn parse_file_path(input: &str) -> IResult<&str, Token> {
        map(
            preceded(
                char('@'),
                take_while1(|c: char| !c.is_whitespace() && c != '@'),
            ),
            |path: &str| Token::File(path.to_string()),
        )(input)
    }

    fn parse_word(input: &str) -> IResult<&str, String> {
        let (input, _) = space0(input)?;
        let (input, word) = recognize(pair(
            take_while1(|c: char| !c.is_whitespace() && c != '@'),
            opt(space1),
        ))(input)?;
        Ok((input, word.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_existing_file() {
        let result =
            Prompt::parse("Please check this file: @src/test_file.txt for content".to_string());

        let prompt = result.unwrap();
        assert_eq!(prompt.files(), vec!["src/test_file.txt"]);
    }

    #[test]
    fn test_with_file_reference_at_end() {
        let result = Prompt::parse("Check this file @src/test_file.txt".to_string());

        let prompt = result.unwrap();
        assert_eq!(prompt.files(), vec!["src/test_file.txt"]);
    }

    #[test]
    fn test_with_unicode_characters() {
        let result = Prompt::parse("Check this Unicode path: @src/测试文件.txt".to_string());

        // Should treat it as a file path and fail since it doesn't exist
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("No such file or directory"));
    }

    #[test]
    fn test_with_escaped_characters() {
        let result = Prompt::parse("Check this path: @src/test\\ file.txt".to_string());

        // Should treat the escaped space as part of the path
        let prompt = result.unwrap();
        assert_eq!(prompt.files(), Vec::<String>::new()); // File doesn't exist
        assert_eq!(prompt.message(), "Check this path: @src/test\\ file.txt");
    }

    #[test]
    fn test_with_special_characters() {
        let result =
            Prompt::parse("Check this path with spaces: @src/path with spaces.txt".to_string());

        // Should treat everything after the space as regular text
        let prompt = result.unwrap();
        assert_eq!(prompt.files(), Vec::<String>::new());
        assert_eq!(
            prompt.message(),
            "Check this path with spaces: @src/path with spaces.txt"
        );
    }

    #[test]
    fn test_with_consecutive_file_references() {
        // This should fail to parse as @ must be preceded by whitespace
        let result = Prompt::parse("@src/a.txt@src/b.txt".to_string());

        // Should treat the entire string as text since the second @ is not properly
        // separated
        let prompt = result.unwrap();
        assert_eq!(prompt.files(), vec!["src/a.txt", "src/b.txt"]);
    }

    #[test]
    fn test_with_duplicate_file_references() {
        let result =
            Prompt::parse("Check this file: @src/test_file.txt @src/test_file.txt".to_string());

        let prompt = result.unwrap();
        assert_eq!(
            prompt.files(),
            vec!["src/test_file.txt", "src/test_file.txt"]
        );
    }

    #[test]
    fn test_with_file_reference_at_start() {
        let result = Prompt::parse("@src/test_file.txt contains some content".to_string());

        let prompt = result.unwrap();
        assert_eq!(prompt.files(), vec!["src/test_file.txt"]);
    }

    #[test]
    fn test_with_malformed_file_reference() {
        let result = Prompt::parse("Check this malformed reference: @ ".to_string());

        // Should fallback to treating the entire input as text
        let error = result.unwrap_err();
        assert_eq!(error, "Check this malformed reference: @ ");
    }

    #[test]
    fn test_with_multiple_files() {
        let result =
            Prompt::parse("Compare @src/test_file.txt with @src/test_file2.txt".to_string());

        let prompt = result.unwrap();
        assert_eq!(
            prompt.files(),
            vec!["src/test_file.txt", "src/test_file2.txt"]
        );
    }

    #[test]
    fn test_with_no_files() {
        let result = Prompt::parse("Just a regular message".to_string());

        let prompt = result.unwrap();
        assert_eq!(prompt.files(), Vec::<String>::new());
        assert_eq!(prompt.message(), "Just a regular message");
    }

    #[test]
    fn test_with_empty_input() {
        let result = Prompt::parse("".to_string());

        let prompt = result.unwrap();
        assert_eq!(prompt.files(), Vec::<String>::new());
        assert_eq!(prompt.message(), "");
    }
}
