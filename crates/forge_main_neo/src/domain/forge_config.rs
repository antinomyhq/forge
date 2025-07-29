use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// A custom command defined in forge.yaml
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct CustomCommand {
    pub name: String,
    pub description: String,
    pub prompt: String,
}

impl CustomCommand {
    /// Create a new custom command
    #[allow(dead_code)]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            prompt: prompt.into(),
        }
    }

    /// Get the command name with the slash prefix
    #[allow(dead_code)]
    pub fn with_slash(&self) -> String {
        format!("/{}", self.name)
    }
}

/// Configuration structure for forge.yaml
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Default)]
#[setters(strip_option, into)]
pub struct ForgeConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CustomCommand>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_walker_depth: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_rules: Option<String>,
}

impl ForgeConfig {
    /// Load forge configuration from a YAML string
    pub fn from_yaml(yaml: &str) -> anyhow::Result<Self> {
        Ok(serde_yml::from_str(yaml)?)
    }

    /// Get all custom commands
    #[allow(dead_code)]
    pub fn custom_commands(&self) -> &[CustomCommand] {
        &self.commands
    }

    /// Find a custom command by name
    #[allow(dead_code)]
    pub fn find_command(&self, name: &str) -> Option<&CustomCommand> {
        self.commands.iter().find(|cmd| cmd.name == name)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_custom_command_new() {
        let fixture = CustomCommand::new("test", "Test command", "Test prompt");
        let actual = fixture.name;
        let expected = "test";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_custom_command_with_slash() {
        let fixture = CustomCommand::new("test", "Test command", "Test prompt");
        let actual = fixture.with_slash();
        let expected = "/test";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_forge_config_from_yaml() {
        let fixture = r#"
commands:
- name: fixme
  description: Looks for all the fixme comments in the code and attempts to fix them
  prompt: Find all the FIXME comments in source-code files and attempt to fix them.
- name: check
  description: Checks if the code is ready to be committed
  prompt: Run lint and test commands
model: anthropic/claude-sonnet-4
max_walker_depth: 1024
"#;
        let actual = ForgeConfig::from_yaml(fixture).unwrap();
        let expected = ForgeConfig {
            commands: vec![
                CustomCommand::new(
                    "fixme",
                    "Looks for all the fixme comments in the code and attempts to fix them",
                    "Find all the FIXME comments in source-code files and attempt to fix them.",
                ),
                CustomCommand::new(
                    "check",
                    "Checks if the code is ready to be committed",
                    "Run lint and test commands",
                ),
            ],
            model: Some("anthropic/claude-sonnet-4".to_string()),
            max_walker_depth: Some(1024),
            custom_rules: None,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_forge_config_find_command() {
        let fixture = ForgeConfig {
            commands: vec![
                CustomCommand::new("fixme", "Fix FIXME comments", "Find and fix FIXME comments"),
                CustomCommand::new("check", "Check code", "Run checks"),
            ],
            ..Default::default()
        };
        let actual = fixture.find_command("fixme");
        let expected_command =
            CustomCommand::new("fixme", "Fix FIXME comments", "Find and fix FIXME comments");
        let expected = Some(&expected_command);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_forge_config_find_command_not_found() {
        let fixture = ForgeConfig::default();
        let actual = fixture.find_command("nonexistent");
        let expected = None;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_forge_config_custom_commands() {
        let fixture = ForgeConfig {
            commands: vec![
                CustomCommand::new("test1", "Test 1", "Prompt 1"),
                CustomCommand::new("test2", "Test 2", "Prompt 2"),
            ],
            ..Default::default()
        };
        let actual = fixture.custom_commands().len();
        let expected = 2;
        assert_eq!(actual, expected);
    }
}
