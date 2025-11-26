#[cfg(test)]
mod tests {
    use crate::model::{ForgeCommandManager, SlashCommand};

    #[tokio::test]
    async fn test_editor_command_parsing() {
        let cmd_manager = ForgeCommandManager::default();

        // Test that /editor is parsed correctly
        let command = cmd_manager.parse("/editor").unwrap();
        assert!(matches!(command, SlashCommand::Editor));

        // Test that /editor with extra content is parsed correctly
        let command = cmd_manager.parse("/editor extra").unwrap();
        assert!(matches!(command, SlashCommand::Editor));
    }

    #[test]
    fn test_editor_command_name() {
        assert_eq!(SlashCommand::Editor.name(), "editor");
    }
}
