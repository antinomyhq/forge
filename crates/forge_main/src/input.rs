use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use forge_api::{Environment, Usage};
use forge_display::TitleFormat;
use tokio::fs;

use crate::console::CONSOLE;
use crate::editor::{ForgeEditor, ReadResult};
use crate::model::{Command, ForgeCommandManager, UserInput};
use crate::prompt::ForgePrompt;
use crate::state::Mode;

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

#[async_trait]
impl UserInput for Console {
    type PromptInput = PromptInput;
    async fn upload<P: Into<PathBuf> + Send>(&self, path: P) -> anyhow::Result<Command> {
        let path = path.into();
        let content = fs::read_to_string(&path).await?.trim().to_string();

        CONSOLE.writeln(content.clone())?;
        Ok(Command::Message(content))
    }

    async fn prompt(&self, input: Option<Self::PromptInput>) -> anyhow::Result<Command> {
        CONSOLE.writeln("")?;

        let mut engine = ForgeEditor::new(self.env.clone(), self.command.clone());
        let prompt: ForgePrompt = input.map(Into::into).unwrap_or_default();

        loop {
            let result = engine.prompt(&prompt);
            match result {
                Ok(ReadResult::Continue) => continue,
                Ok(ReadResult::Exit) => return Ok(Command::Exit),
                Ok(ReadResult::Empty) => continue,
                Ok(ReadResult::Success(text)) => {
                    tokio::spawn(
                        crate::ui::TRACKER.dispatch(forge_tracker::EventKind::Prompt(text.clone())),
                    );
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
                Err(e) => {
                    CONSOLE.writeln(TitleFormat::failed(e.to_string()).format())?;
                }
            }
        }
    }
}

pub enum PromptInput {
    Update {
        title: Option<String>,
        usage: Option<Usage>,
        mode: Mode,
    },
}

impl From<PromptInput> for ForgePrompt {
    fn from(input: PromptInput) -> Self {
        match input {
            PromptInput::Update { title, usage, mode } => {
                let mut prompt = ForgePrompt::default();
                prompt.mode(mode);
                if let Some(title) = title {
                    prompt.title(title);
                }
                if let Some(usage) = usage {
                    prompt.usage(usage);
                }
                prompt
            }
        }
    }
}
