use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use forge_api::Environment;
use forge_display::TitleFormat;
use tokio::fs;

use crate::editor::{ForgeEditor, ReadResult};
use crate::model::{Command, ForgeCommandManager};
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
    pub async fn upload<P: Into<PathBuf> + Send>(&self, path: P) -> anyhow::Result<Command> {
        let path = path.into();
        let content = fs::read_to_string(&path).await?.trim().to_string();

        println!("{}", content.clone());
        Ok(Command::Message(content))
    }

    pub async fn prompt(&self, prompt: ForgePrompt) -> anyhow::Result<Command> {
        let engine = Mutex::new(ForgeEditor::new(self.env.clone(), self.command.clone()));

        loop {
            let mut forge_editor = engine.lock().unwrap();
            let user_input = forge_editor.prompt(&prompt)?;
            drop(forge_editor);
            match user_input {
                ReadResult::Continue => continue,
                ReadResult::Exit => return Ok(Command::Exit),
                ReadResult::Empty => continue,
                ReadResult::Success(text) => {
                    tracker::prompt(text.clone());
                    match self.command.parse(&text) {
                        Ok(command) => return Ok(command),
                        Err(error) => {
                            tracing::error!(error = ?error);
                            eprintln!("{}", TitleFormat::error(error.to_string()));
                        }
                    }
                }
            }
        }
    }
}
