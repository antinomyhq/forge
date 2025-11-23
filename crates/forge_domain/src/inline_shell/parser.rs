use anyhow::Result;

use crate::inline_shell::InlineShellError;

/// Represents a detected inline shell command with its position information
#[derive(Debug, Clone, PartialEq)]
pub struct InlineShellCommand {
    /// The full match including ![...] syntax
    pub full_match: String,
    /// The actual command to execute (without ![...] wrapper)
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

/// Parses content to find all inline shell commands
pub fn parse_inline_commands(content: &str) -> Result<ParsedContent, InlineShellError> {
    let mut commands = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '!' && chars[i + 1] == '[' {
            // Found potential command start
            let mut bracket_count = 1;
            let mut j = i + 2;

            while j < chars.len() && bracket_count > 0 {
                match chars[j] {
                    '[' => bracket_count += 1,
                    ']' => bracket_count -= 1,
                    _ => {}
                }
                j += 1;
            }

            if bracket_count == 0 {
                // Found complete command
                let command_start = i + 2;
                let command_end = j - 1;

                if command_start < command_end {
                    let command: String = chars[command_start..command_end].iter().collect();
                    let command = command.trim();

                    if !command.is_empty() {
                        let full_match: String = chars[i..j].iter().collect();
                        let end_pos = if j < chars.len() && chars[j] == ' ' {
                            j + 1
                        } else {
                            j
                        };
                        commands.push(InlineShellCommand {
                            full_match,
                            command: command.to_string(),
                            start_pos: i + 1, // Skip '!'
                            end_pos,          // Include trailing space if exists
                        });
                    }
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }

    Ok(ParsedContent {
        original_content: content.to_string(),
        commands_found: commands,
    })
}
