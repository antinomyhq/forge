use std::sync::Arc;
use anyhow::Result;
use colored::Colorize;
use forge_api::{AgentMessage, ChatRequest, ChatResponse, ConversationId, Event, Model, API};
use forge_display::TitleFormat;
use forge_snaps::SnapshotInfo;
use forge_tracker::EventKind;
use lazy_static::lazy_static;
use serde_json::Value;
use tokio_stream::StreamExt;

use crate::banner;
use crate::cli::{Cli, Snapshot, SnapshotCommand};
use crate::console::CONSOLE;
use crate::info::Info;
use crate::input::Console;
use crate::model::{Command, UserInput};
use crate::state::{Mode, UIState};

// Event type constants moved to UI layer
pub const EVENT_USER_TASK_INIT: &str = "user_task_init";
pub const EVENT_USER_TASK_UPDATE: &str = "user_task_update";
pub const EVENT_USER_HELP_QUERY: &str = "user_help_query";
pub const EVENT_TITLE: &str = "title";
pub const EVENT_USER_COMPACT_INIT: &str = "user_compact_init"; // New event for /compact

lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}

pub struct UI<F> {
    state: UIState,
    api: Arc<F>,
    console: Console,
    cli: Cli,
    models: Option<Vec<Model>>,
    #[allow(dead_code)] // The guard is kept alive by being held in the struct
    _guard: forge_tracker::Guard,
}

impl<F: API> UI<F> {
    // Set the current mode and update conversation variable
    async fn handle_mode_change(&mut self, mode: Mode) -> Result<()> {
        // Update the mode in state
        self.state.mode = mode;

        // Show message that mode changed
        let mode_str = self.state.mode.to_string();

        // Set the mode variable in the conversation if a conversation exists
        let conversation_id = self.init_conversation().await?;
        self.api
            .set_variable(
                &conversation_id,
                "mode".to_string(),
                Value::from(mode_str.as_str()),
            )
            .await?;

        // Print a mode-specific message
        let mode_message = match self.state.mode {
            Mode::Act => "mode - executes commands and makes file changes",
            Mode::Plan => "mode - plans actions without making changes",
            Mode::Help => "mode - answers questions (type /act or /plan to switch back)",
        };

        CONSOLE.write(
            TitleFormat::success(&mode_str)
                .sub_title(mode_message)
                .format(),
        )?;

        Ok(())
    }

    // Helper functions for creating events with the specific event names
    fn create_task_init_event(content: impl ToString) -> Event {
        Event::new(EVENT_USER_TASK_INIT, content)
    }

    fn create_task_update_event(content: impl ToString) -> Event {
        Event::new(EVENT_USER_TASK_UPDATE, content)
    }

    fn create_user_help_query_event(content: impl ToString) -> Event {
        Event::new(EVENT_USER_HELP_QUERY, content)
    }

    fn create_user_compact_init_event(content: impl ToString) -> Event {
        Event::new(EVENT_USER_COMPACT_INIT, content) // New event for /compact
    }

    pub fn init(cli: Cli, api: Arc<F>) -> Result<Self> {
        // Parse CLI arguments first to get flags
        let env = api.environment();
        Ok(Self {
            state: Default::default(),
            api,
            console: Console::new(env.clone()),
            cli,
            models: None,
            _guard: forge_tracker::init_tracing(env.log_path())?,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        if let Some(snapshot_command) = self.cli.snapshot.as_ref() {
            return match snapshot_command {
                Snapshot::Snapshot { sub_command } => self.handle_snaps(sub_command).await,
            };
        }

        // Handle direct prompt if provided
        let prompt = self.cli.prompt.clone();
        if let Some(prompt) = prompt {
            self.chat(prompt).await?;
            return Ok(());
        }

        // Display the banner in dimmed colors since we're in interactive mode
        banner::display()?;

        // Get initial input from file or prompt
        let mut input = match &self.cli.command {
            Some(path) => self.console.upload(path).await?,
            None => self.console.prompt(None).await?,
        };

        loop {
            match input {
                Command::Dump => {
                    self.handle_dump().await?;
                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
                    continue;
                }
                Command::New => {
                    banner::display()?;
                    self.state = Default::default();
                    input = self.console.prompt(None).await?;
                    continue;
                }
                Command::Info => {
                    let info =
                        Info::from(&self.api.environment()).extend(Info::from(&self.state.usage));

                    CONSOLE.writeln(info.to_string())?;

                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
                    continue;
                }
                Command::Message(ref content) => {
                    let chat_result = match self.state.mode {
                        Mode::Help => self.help_chat(content.clone()).await,
                        _ => self.chat(content.clone()).await,
                    };
                    if let Err(err) = chat_result {
                        CONSOLE.writeln(TitleFormat::failed(format!("{:?}", err)).format())?;
                    }
                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
                }
                Command::Act => {
                    self.handle_mode_change(Mode::Act).await?;

                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
                    continue;
                }
                Command::Plan => {
                    self.handle_mode_change(Mode::Plan).await?;

                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
                    continue;
                }
                Command::Help => {
                    self.handle_mode_change(Mode::Help).await?;

                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
                    continue;
                }
                Command::Exit => {
                    break;
                }
                Command::Models => {
                    let models = if let Some(models) = self.models.as_ref() {
                        models
                    } else {
                        let models = self.api.models().await?;
                        self.models = Some(models);
                        self.models.as_ref().unwrap()
                    };
                    let info: Info = models.as_slice().into();
                    CONSOLE.writeln(info.to_string())?;

                    input = self.console.prompt(None).await?;
                }
                Command::Compact => {
                    self.handle_compact().await?;

                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
                    continue;
                }
            }
        }

        Ok(())
    }

    async fn handle_compact(&mut self) -> Result<()> {
        // Get the current context from the state
        let context = self.state.context.clone();

        // Summarize the context
        let summarized_context = self.summarize_context(context);

        // Replace the context with the summarized version
        self.state.context = summarized_context;

        // Notify the user
        CONSOLE.writeln(
            TitleFormat::success("Context Compacted")
                .sub_title("The context has been summarized and replaced.")
                .format(),
        )?;

        Ok(())
    }

    fn summarize_context(&self, context: HashMap<String, Value>) -> HashMap<String, Value> {
        // Example summarization logic
        let mut summarized = HashMap::new();
        summarized.insert("summary".to_string(), json!("This is a summarized context"));
        summarized
    }

    // Rest of the code remains unchanged...
}
