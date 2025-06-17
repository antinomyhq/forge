use std::path::PathBuf;
use std::sync::Arc;

use forge_api::Environment;
use forge_display::TitleFormat;
use tokio::fs;

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

        println!("{}", content.clone());
        Ok(Command::Message(content))
    }

    pub async fn prompt(&self, prompt: ForgePrompt) -> anyhow::Result<Command> {
        let mut engine = ForgeEditor::new(self.env.clone(), self.command.clone());
        loop {
            let result = engine.prompt(&prompt)?;
            if let Some(command) = self.process_read(result).await? {
                return Ok(command);
            };
        }
    }
    pub async fn process_read(&self, result: ReadResult) -> anyhow::Result<Option<Command>> {
        match result {
            ReadResult::Continue => (),
            ReadResult::Exit => return Ok(Some(Command::Exit)),
            ReadResult::Empty => (),
            ReadResult::Success(text) => {
                tokio::spawn(TRACKER.dispatch(forge_tracker::EventKind::Prompt(text.clone())));
                match self.command.parse(&text) {
                    Ok(command) => return Ok(Some(command)),
                    Err(error) => {
                        tracing::error!(error = ?error);
                        eprintln!("{}", TitleFormat::error(error.to_string()));
                    }
                }
            }
        }

        Ok(None)
    }
}
