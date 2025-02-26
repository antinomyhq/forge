use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use forge_api::{Usage, API};
use forge_display::TitleFormat;

use crate::console::CONSOLE;
use crate::editor::{ForgeEditor, ReadResult};
use crate::model::{Command, UserInput};
use crate::prompt::ForgePrompt;

/// Console implementation for handling user input via command line.
#[derive(Debug)]
pub struct Console<F> {
    api: Arc<F>,
}

impl<F> Console<F> {
    /// Creates a new instance of `Console`.
    pub fn new(api: Arc<F>) -> Self {
        Self { api }
    }
}

#[async_trait]
impl<F: API + Send + Sync> UserInput for Console<F> {
    type PromptInput = PromptInput;
    async fn upload<P: Into<PathBuf> + Send>(&self, path: P) -> anyhow::Result<Command> {
        let path = path.into();
        let content = self.api.read_file(&path).await?.trim().to_string();

        CONSOLE.writeln(content.clone())?;
        Ok(Command::Message(content))
    }

    async fn prompt(&self, input: Option<Self::PromptInput>) -> anyhow::Result<Command> {
        CONSOLE.writeln("")?;
        let env = self.api.environment();
        let mut engine = ForgeEditor::start(env);
        let prompt: ForgePrompt = input.map(Into::into).unwrap_or_default();

        loop {
            let result = engine.prompt(&prompt);
            match result {
                Ok(ReadResult::Continue) => continue,
                Ok(ReadResult::Exit) => return Ok(Command::Exit),
                Ok(ReadResult::Empty) => continue,
                Ok(ReadResult::Success(text)) => {
                    return Ok(Command::parse(&text));
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
    },
}

impl From<PromptInput> for ForgePrompt {
    fn from(input: PromptInput) -> Self {
        match input {
            PromptInput::Update { title, usage } => {
                let mut prompt = ForgePrompt::default();
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
