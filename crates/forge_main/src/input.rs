use std::path::PathBuf;
use std::sync::Arc;

use forge_api::Environment;
use forge_display::TitleFormat;
use tokio::fs;

use crate::console::CONSOLE;
use crate::editor::{ForgeEditor, ReadResult};
use crate::model::{Command, ForgeCommandManager};
use crate::prompt::ForgePrompt;
use crate::TRACKER;

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
    pub async fn upload<P: Into<PathBuf> + Send>(&self, path: P) -> anyhow::Result<Command> {
        let path = path.into();
        let content = fs::read_to_string(&path).await?.trim().to_string();

        CONSOLE.writeln(content.clone())?;
        Ok(Command::Message(content))
    }

    pub async fn prompt(&self, prompt: Option<ForgePrompt>) -> anyhow::Result<Command> {
        CONSOLE.writeln("")?;

        let mut engine = ForgeEditor::new(self.env.clone(), self.command.clone());
        let prompt: ForgePrompt = prompt.unwrap_or_default();

        loop {
            let result = engine.prompt(&prompt)?;
            match result {
                ReadResult::Continue => continue,
                ReadResult::Exit => return Ok(Command::Exit),
                ReadResult::Empty => continue,
                ReadResult::Success(text) => {
                    tokio::spawn(TRACKER.dispatch(forge_tracker::EventKind::Prompt(text.clone())));
                    match self.command.parse(&text) {
                        Ok(command) => return Ok(command),
                        Err(e) => {
                            CONSOLE.writeln(
                                TitleFormat::failed("command")
                                    .sub_title(e.to_string())
                                    .format(),
                            )?;
                        }
                    }
                }
            }
        }
    }
}
