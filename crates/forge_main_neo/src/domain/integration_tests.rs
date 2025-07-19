#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;
    use tokio::fs::write;

    use crate::domain::editor_helpers::EditorStateExt;
    use crate::domain::{CustomCommand, ForgeConfigService};

    #[tokio::test]
    async fn test_integration_forge_config_loading() {
        // Create a temporary forge.yaml file
        let temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
commands:
- name: fixme
  description: Looks for all the fixme comments in the code and attempts to fix them
  prompt: Find all the FIXME comments in source-code files and attempt to fix them.
- name: pr-description
  description: Updates the description of the PR
  prompt: |-
    - I have created a Pull Request with all the accepted changes
    - Understand the current PR deeply using the GH CLI and update the PR title and description
    - Make sure the title follows conventional commits standard
    - Top-level summary should contain 2-3 lines about the core functionality improvements
- name: check
  description: Checks if the code is ready to be committed
  prompt: |-
    - Run the `lint` and `test` commands and verify if everything is fine.
      <lint>cargo +nightly fmt --all; cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace</lint>
      <test>cargo insta test --accept --unreferenced=delete</test>
    - Fix every issue found in the process
model: anthropic/claude-sonnet-4
max_walker_depth: 1024
"#;
        write(temp_file.path(), config_content).await.unwrap();

        // Load the config
        let actual = ForgeConfigService::load_config_from_path(temp_file.path())
            .await
            .unwrap();

        // Verify the config was loaded correctly
        assert_eq!(actual.commands.len(), 3);
        assert_eq!(actual.commands[0].name, "fixme");
        assert_eq!(actual.commands[1].name, "pr-description");
        assert_eq!(actual.commands[2].name, "check");
        assert_eq!(actual.model, Some("anthropic/claude-sonnet-4".to_string()));
        assert_eq!(actual.max_walker_depth, Some(1024));
    }

    #[test]
    fn test_spotlight_command_integration() {
        use crate::domain::spotlight::{SpotlightCommand, SpotlightState};

        // Create custom commands
        let custom_commands = vec![
            CustomCommand {
                name: "fixme".to_string(),
                description: "Fix FIXME comments".to_string(),
                prompt: "Find and fix FIXME comments".to_string(),
            },
            CustomCommand {
                name: "check".to_string(),
                description: "Check code".to_string(),
                prompt: "Run checks".to_string(),
            },
        ];

        // Create spotlight state and set custom commands
        let mut spotlight_state = SpotlightState::default();
        spotlight_state.set_custom_commands(custom_commands);

        // Test that all commands are available
        let all_commands = spotlight_state.all_commands();
        assert_eq!(all_commands.len(), 14); // 12 slash commands + 2 custom commands

        // Test filtering for custom commands
        spotlight_state
            .editor
            .set_text_insert_mode("fix".to_string());
        let filtered_commands = spotlight_state.filtered_commands();
        assert_eq!(filtered_commands.len(), 1);

        match &filtered_commands[0] {
            SpotlightCommand::Custom(cmd) => {
                assert_eq!(cmd.name, "fixme");
                assert_eq!(cmd.description, "Fix FIXME comments");
            }
            _ => panic!("Expected custom command"),
        }
    }
}
