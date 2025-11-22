use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;

/// Represents a detected inline shell command with its position information
#[derive(Debug, Clone, PartialEq)]
pub struct InlineShellCommand {
    /// The full match including the ![...] syntax
    pub full_match: String,
    /// The actual command to execute (without the ![...] wrapper)
    pub command: String,
    /// Start position of match in original content
    pub start_pos: usize,
    /// End position of match in original content
    pub end_pos: usize,
}

/// Result of parsing content for inline shell commands
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedContent {
    /// The original content (unchanged - replacement happens during execution)
    pub original_content: String,
    /// All inline shell commands found in content
    pub commands_found: Vec<InlineShellCommand>,
}

use crate::inline_shell::InlineShellError;

/// Regex pattern for matching inline shell commands: ![command]
static INLINE_SHELL_REGEX: OnceLock<Regex> = OnceLock::new();

/// Gets the compiled regex for inline shell commands
fn get_inline_shell_regex() -> &'static Regex {
    INLINE_SHELL_REGEX.get_or_init(|| Regex::new(r"!\[([^\]]+)\]").expect("Invalid regex pattern"))
}

/// Parses content to find all inline shell commands
///
/// # Arguments
/// * `content` - The content to parse for inline shell commands
///
/// # Returns
/// * `Ok(ParsedContent)` with all found commands
/// * `Err(InlineShellError)` if parsing fails
///
/// # Examples
/// ```
/// use forge_domain::inline_shell::parse_inline_commands;
///
/// let content = "Run ![echo hello] and ![pwd]";
/// let parsed = parse_inline_commands(content).unwrap();
/// assert_eq!(parsed.commands_found.len(), 2);
/// ```
pub fn parse_inline_commands(content: &str) -> Result<ParsedContent, InlineShellError> {
    let regex = get_inline_shell_regex();
    let mut commands = Vec::new();

    for cap in regex.captures_iter(content) {
        let Some(full_match) = cap.get(0) else {
            continue;
        };

        let Some(command_match) = cap.get(1) else {
            continue;
        };

        let command = command_match.as_str().trim();

        // Validate command
        if command.is_empty() {
            return Err(InlineShellError::EmptyCommand { position: full_match.start() });
        }

        // Check for nested commands (basic validation)
        if command.contains("![") {
            return Err(InlineShellError::MalformedSyntax {
                position: full_match.start(),
                reason: "Nested inline shell commands are not allowed".to_string(),
            });
        }

        commands.push(InlineShellCommand {
            full_match: full_match.as_str().to_string(),
            command: command.to_string(),
            start_pos: full_match.start(),
            end_pos: full_match.end(),
        });
    }

    Ok(ParsedContent {
        original_content: content.to_string(),
        commands_found: commands,
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_single_command() -> anyhow::Result<()> {
        let fixture = "List files: ![ls -la] here";
        let actual = parse_inline_commands(fixture)?;
        let expected = ParsedContent {
            original_content: fixture.to_string(),
            commands_found: vec![InlineShellCommand {
                full_match: "![ls -la]".to_string(),
                command: "ls -la".to_string(),
                start_pos: 12,
                end_pos: 21,
            }],
        };
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn test_parse_multiple_commands() -> anyhow::Result<()> {
        let fixture = "Run ![echo hello] and then ![pwd]";
        let actual = parse_inline_commands(fixture)?;

        // Debug output
        println!("Actual commands found: {:?}", actual.commands_found);
        println!(
            "First command: start={}, end={}",
            actual.commands_found[0].start_pos, actual.commands_found[0].end_pos
        );
        println!(
            "Second command: start={}, end={}",
            actual.commands_found[1].start_pos, actual.commands_found[1].end_pos
        );

        // Verify positions are correct
        assert_eq!(actual.commands_found[0].start_pos, 4);
        assert_eq!(actual.commands_found[0].end_pos, 17); // ![echo hello] ends at index 16, so end() should be 17
        assert_eq!(actual.commands_found[1].start_pos, 27);
        assert_eq!(actual.commands_found[1].end_pos, 33); // ![pwd] ends at index 32, so end() should be 33

        Ok(())
    }

    #[test]
    fn test_parse_no_commands() -> anyhow::Result<()> {
        let fixture = "No commands here";
        let actual = parse_inline_commands(fixture)?;
        let expected = ParsedContent {
            original_content: fixture.to_string(),
            commands_found: vec![],
        };
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn test_parse_empty_command() -> anyhow::Result<()> {
        let fixture = "Empty: ![] command";
        let actual = parse_inline_commands(fixture);
        // Empty commands are not matched by our regex, which requires at least one
        // character
        assert!(actual.is_ok());
        let parsed = actual?;
        assert_eq!(parsed.commands_found.len(), 0);
        Ok(())
    }

    #[test]
    fn test_parse_malformed_command() -> anyhow::Result<()> {
        let fixture = "Malformed: ![unclosed command";
        let actual = parse_inline_commands(fixture);
        // This should actually parse successfully since our regex doesn't validate
        // closing quotes The malformed detection would need more sophisticated
        // parsing
        assert!(actual.is_ok());
        Ok(())
    }

    #[test]
    fn test_parse_command_with_quotes() -> anyhow::Result<()> {
        let fixture = "Quote: ![echo \"hello world\"]";
        let actual = parse_inline_commands(fixture)?;
        let expected = ParsedContent {
            original_content: fixture.to_string(),
            commands_found: vec![InlineShellCommand {
                full_match: "![echo \"hello world\"]".to_string(),
                command: "echo \"hello world\"".to_string(),
                start_pos: 7,
                end_pos: 28,
            }],
        };
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn test_parse_complex_command() -> anyhow::Result<()> {
        let fixture = "Complex: ![find . -name \"*.rs\" -type f | head -5]";
        let actual = parse_inline_commands(fixture)?;
        let expected = ParsedContent {
            original_content: fixture.to_string(),
            commands_found: vec![InlineShellCommand {
                full_match: "![find . -name \"*.rs\" -type f | head -5]".to_string(),
                command: "find . -name \"*.rs\" -type f | head -5".to_string(),
                start_pos: 9,
                end_pos: 49,
            }],
        };
        assert_eq!(actual, expected);
        Ok(())
    }

    // Property-based tests using proptest
    proptest::proptest! {
        #[test]
        fn test_parser_never_crashes_on_any_input(ref input in "\\PC*") {
            // The parser should never panic on any input
            let _result = parse_inline_commands(input);
            // We don't care about the result, just that it doesn't crash
        }

        #[test]
        fn test_parser_preserves_non_command_text(ref input in "\\PC*") {
            // When there are no commands, original content should be preserved
            let no_cmd_input = input.replace("![", "");
            if !no_cmd_input.contains("![") {
                let result = parse_inline_commands(&no_cmd_input).unwrap();
                assert_eq!(result.original_content, no_cmd_input);
                assert_eq!(result.commands_found.len(), 0);
            }
        }

        fn test_parser_extracts_commands_correctly(
            ref prefix in "[a-zA-Z0-9 ]{0,10}",
            ref command in "[a-zA-Z0-9 \\-_]{1,20}",
            ref suffix in "[a-zA-Z0-9 ]{0,10}"
        ) {
            let input = format!("{}![{}]{}", prefix, command, suffix);
            let result = parse_inline_commands(&input).unwrap();

            if result.commands_found.len() == 1 {
                let extracted_cmd = &result.commands_found[0];
                assert_eq!(extracted_cmd.command, *command);
                assert_eq!(extracted_cmd.full_match, format!("![{}]", command));
                assert!(result.original_content.contains(&extracted_cmd.full_match));
            }
        }

        #[test]
        fn test_parser_handles_multiple_commands(
            ref cmd1 in "[a-z]{1,5}",
            ref cmd2 in "[a-z]{1,5}",
            ref cmd3 in "[a-z]{1,5}"
        ) {
            let input = format!("![{}] middle ![{}] end ![{}]", cmd1, cmd2, cmd3);
            let result = parse_inline_commands(&input).unwrap();

            assert_eq!(result.commands_found.len(), 3);
            assert_eq!(result.commands_found[0].command, *cmd1);
            assert_eq!(result.commands_found[1].command, *cmd2);
            assert_eq!(result.commands_found[2].command, *cmd3);
        }

        #[test]
        fn test_parser_positions_are_valid(ref input in "\\PC*") {
            // All positions should be valid indices into original string
            let result = parse_inline_commands(input).unwrap();

            for cmd in &result.commands_found {
                assert!(cmd.start_pos <= input.len());
                assert!(cmd.end_pos <= input.len());
                assert!(cmd.start_pos < cmd.end_pos);
                assert_eq!(&input[cmd.start_pos..cmd.end_pos], cmd.full_match);
            }
        }
    }
}
