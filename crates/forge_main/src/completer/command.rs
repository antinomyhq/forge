use reedline::{Completer, Span, Suggestion};

use crate::model::Command;

#[derive(Default)]
pub struct CommandCompleter;

impl Completer for CommandCompleter {
    fn complete(&mut self, line: &str, _: usize) -> Vec<reedline::Suggestion> {
        let mut commands = Command::available_commands();
        commands.push("/retry".to_string()); 
        commands
            .into_iter()
            .filter(|cmd| cmd.starts_with(line))
            .map(|cmd| Suggestion {
                value: cmd,
                description: None,
                style: None,
                extra: None,
                span: Span::new(0, line.len()),
                append_whitespace: true,
            })
            .collect()
    }
}
