#[cfg(test)]
mod tests {
    use clap::Parser;
    use forge_main::{Cli, ConversationCommand, TopLevelCommand};

    #[test]
    fn test_rename_with_multiple_words_without_quotes() {
        let fixture = Cli::try_parse_from([
            "forge",
            "conversation",
            "rename",
            "abc123",
            "ExludeExlude",
            "fsdfsdf",
        ])
        .unwrap();

        let command = match fixture.subcommands.unwrap() {
            TopLevelCommand::Conversation(conv_cmd) => conv_cmd,
            _ => panic!("Expected conversation command"),
        };

        match command.command {
            ConversationCommand::Rename { id, new_title } => {
                assert_eq!(id, "abc123");
                assert_eq!(
                    new_title,
                    Some(vec!["ExludeExlude".to_string(), "fsdfsdf".to_string()])
                );
            }
            _ => panic!("Expected rename command"),
        }
    }

    #[test]
    fn test_rename_with_single_word() {
        let fixture =
            Cli::try_parse_from(["forge", "conversation", "rename", "abc123", "AABBCC"]).unwrap();

        let command = match fixture.subcommands.unwrap() {
            TopLevelCommand::Conversation(conv_cmd) => conv_cmd,
            _ => panic!("Expected conversation command"),
        };

        match command.command {
            ConversationCommand::Rename { id, new_title } => {
                assert_eq!(id, "abc123");
                assert_eq!(new_title, Some(vec!["AABBCC".to_string()]));
            }
            _ => panic!("Expected rename command"),
        }
    }

    #[test]
    fn test_rename_without_title() {
        let fixture = Cli::try_parse_from(["forge", "conversation", "rename", "abc123"]).unwrap();

        let command = match fixture.subcommands.unwrap() {
            TopLevelCommand::Conversation(conv_cmd) => conv_cmd,
            _ => panic!("Expected conversation command"),
        };

        match command.command {
            ConversationCommand::Rename { id, new_title } => {
                assert_eq!(id, "abc123");
                assert_eq!(new_title, None);
            }
            _ => panic!("Expected rename command"),
        }
    }

    #[test]
    fn test_rename_with_multiple_words_still_works() {
        let fixture = Cli::try_parse_from([
            "forge",
            "conversation",
            "rename",
            "abc123",
            "quoted title with spaces",
        ])
        .unwrap();

        let command = match fixture.subcommands.unwrap() {
            TopLevelCommand::Conversation(conv_cmd) => conv_cmd,
            _ => panic!("Expected conversation command"),
        };

        match command.command {
            ConversationCommand::Rename { id, new_title } => {
                assert_eq!(id, "abc123");
                assert_eq!(
                    new_title,
                    Some(vec!["quoted title with spaces".to_string()])
                );
            }
            _ => panic!("Expected rename command"),
        }
    }
}
