use anyhow::{Context, Result};
use rust_embed::RustEmbed;

/// Embedded built-in command markdown files
#[derive(RustEmbed)]
#[folder = "src/built_in_commands/"]
#[include = "*.md"]
struct BuiltInCommands;

/// Simple struct to hold parsed command information
pub struct CommandInfo {
    pub name: String,
    pub description: String,
}

/// Parse a command from markdown content with YAML frontmatter
fn parse_command_markdown(content: &str) -> Result<CommandInfo> {
    use gray_matter::Matter;
    use gray_matter::engine::YAML;

    #[derive(serde::Deserialize)]
    struct FrontMatter {
        name: String,
        description: String,
    }

    let matter = Matter::<YAML>::new();
    let result = matter.parse::<FrontMatter>(content)?;

    let front_matter = result.data.context("Missing command frontmatter")?;

    Ok(CommandInfo {
        name: front_matter.name,
        description: front_matter.description,
    })
}

/// Get all built-in commands
///
/// # Errors
///
/// Returns error if any command file cannot be loaded or parsed
pub fn get_built_in_commands() -> Result<Vec<CommandInfo>> {
    let mut commands = Vec::new();

    for file in BuiltInCommands::iter() {
        let content = BuiltInCommands::get(&file)
            .with_context(|| format!("Failed to load built-in command: {}", file))?;

        let content_str = std::str::from_utf8(content.data.as_ref())
            .with_context(|| format!("Invalid UTF-8 in command file: {}", file))?;

        let command = parse_command_markdown(content_str)
            .with_context(|| format!("Failed to parse built-in command: {}", file))?;

        commands.push(command);
    }

    Ok(commands)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_get_built_in_commands() {
        let actual = get_built_in_commands().unwrap();

        // Should have all 20 built-in commands
        assert_eq!(actual.len(), 20);

        // Verify a few key commands exist
        assert!(actual.iter().any(|c| c.name == "info"));
        assert!(actual.iter().any(|c| c.name == "agent"));
        assert!(actual.iter().any(|c| c.name == "sync"));
        assert!(actual.iter().any(|c| c.name == "commit"));
    }

    #[test]
    fn test_command_has_description() {
        let actual = get_built_in_commands().unwrap();

        // All commands should have non-empty descriptions
        for command in actual {
            assert!(!command.name.is_empty(), "Command name should not be empty");
            assert!(
                !command.description.is_empty(),
                "Command {} should have a description",
                command.name
            );
        }
    }
}
