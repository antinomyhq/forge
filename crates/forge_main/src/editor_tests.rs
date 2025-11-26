#[cfg(test)]
mod tests {
    use crate::model::{ForgeCommandManager, SlashCommand};

    #[tokio::test]
    async fn test_editor_command_parsing() {
        let cmd_manager = ForgeCommandManager::default();

        // Test that /edit is parsed correctly
        let command = cmd_manager.parse("/edit").unwrap();
        assert!(matches!(command, SlashCommand::Editor));

        // Test that /edit with extra content is parsed correctly
        let command = cmd_manager.parse("/edit extra").unwrap();
        assert!(matches!(command, SlashCommand::Editor));
    }

    #[test]
    fn test_editor_command_name() {
        assert_eq!(SlashCommand::Editor.name(), "edit");
    }
}
