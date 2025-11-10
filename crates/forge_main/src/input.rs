use std::sync::{Arc, Mutex};

use forge_api::Environment;

use crate::editor::{ForgeEditor, ReadResult};
use crate::model::{ForgeCommandManager, SlashCommand};
use crate::prompt::ForgePrompt;
use crate::tracker;

/// Console implementation for handling user input via command line.
#[derive(Debug)]
pub struct Console {
    env: Environment,
    command: Arc<ForgeCommandManager>,
}

impl Console {
    /// Creates a new instance of `Console`.
    pub fn new(env: Environment, command: Arc<ForgeCommandManager>) -> Self {
        Self { env, command }
    }
}

impl Console {
    pub async fn prompt(&self, prompt: ForgePrompt) -> anyhow::Result<SlashCommand> {
        let engine = Mutex::new(ForgeEditor::new(self.env.clone(), self.command.clone()));

        loop {
            let mut forge_editor = engine.lock().unwrap();
            let user_input = forge_editor.prompt(&prompt)?;
            drop(forge_editor);
            match user_input {
                ReadResult::Continue => continue,
                ReadResult::Exit => return Ok(SlashCommand::Exit),
                ReadResult::Empty => continue,
                ReadResult::Success(text) => {
                    tracker::prompt(text.clone());
                    return self.command.parse(&text);
                }
            }
        }
    }
}
