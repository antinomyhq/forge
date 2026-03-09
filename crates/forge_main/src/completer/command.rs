use std::sync::Arc;

use reedline::{Completer, Span, Suggestion};

use crate::model::ForgeCommandManager;

#[derive(Clone)]
pub struct CommandCompleter(Arc<ForgeCommandManager>);

impl CommandCompleter {
    pub fn new(command_manager: Arc<ForgeCommandManager>) -> Self {
        Self(command_manager)
    }
}

impl Completer for CommandCompleter {
    fn complete(&mut self, line: &str, _: usize) -> Vec<reedline::Suggestion> {
        self.0
            .list()
            .into_iter()
            .filter_map(|cmd| {
                // For command completion, we want to show commands with `/` prefix
                let display_name = if cmd.name.starts_with('!') {
                    // Shell commands already have the `!` prefix
                    cmd.name.clone()
                } else {
                    // Add `/` prefix for slash commands
                    format!("/{}", cmd.name)
                };

                // Check if the display name starts with what the user typed
                if display_name.starts_with(line) {
                    Some(Suggestion {
                        value: display_name,
                        description: Some(cmd.description),
                        style: None,
                        extra: None,
                        span: Span::new(0, line.len()),
                        append_whitespace: false,
                        match_indices: None,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}
