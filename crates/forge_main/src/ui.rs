use std::sync::Arc;

use anyhow::Result;
use colored::Colorize;
use forge_api::{AgentMessage, ChatRequest, ChatResponse, ConversationId, Event, Model, API};
use forge_display::TitleFormat;
use forge_tracker::EventKind;
use lazy_static::lazy_static;
use serde_json::Value;
use tokio_stream::StreamExt;

use crate::banner;
use crate::cli::Cli;
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
            }
        }

        Ok(())
    }

    async fn init_conversation(&mut self) -> Result<ConversationId> {
        match self.state.conversation_id {
            Some(ref id) => Ok(id.clone()),
            None => {
                let conversation_id = self
                    .api
                    .init(self.api.load(self.cli.workflow.as_deref()).await?)
                    .await?;

                self.state.conversation_id = Some(conversation_id.clone());

                Ok(conversation_id)
            }
        }
    }

    async fn chat(&mut self, content: String) -> Result<()> {
        let conversation_id = self.init_conversation().await?;

        // Determine if this is the first message or an update based on conversation
        // history
        let conversation = self.api.conversation(&conversation_id).await?;

        // Create a ChatRequest with the appropriate event type
        let event = if conversation
            .as_ref()
            .is_none_or(|c| c.rfind_event(EVENT_USER_TASK_INIT).is_none())
        {
            Self::create_task_init_event(content.clone())
        } else {
            Self::create_task_update_event(content.clone())
        };

        // Create the chat request with the event
        let chat = ChatRequest::new(event, conversation_id);

        tokio::spawn(TRACKER.dispatch(EventKind::Prompt(content)));

        match self.api.chat(chat).await {
            Ok(mut stream) => self.handle_chat_stream(&mut stream).await,
            Err(err) => Err(err),
        }
    }

    async fn handle_chat_stream(
        &mut self,
        stream: &mut (impl StreamExt<Item = Result<AgentMessage<ChatResponse>>> + Unpin),
    ) -> Result<()> {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    return Ok(());
                }
                maybe_message = stream.next() => {
                    match maybe_message {
                        Some(Ok(message)) => self.handle_chat_response(message)?,
                        Some(Err(err)) => {
                            return Err(err);
                        }
                        None => return Ok(()),
                    }
                }
            }
        }
    }

    async fn handle_dump(&mut self) -> Result<()> {
        if let Some(conversation_id) = self.state.conversation_id.clone() {
            let conversation = self.api.conversation(&conversation_id).await?;
            if let Some(conversation) = conversation {
                let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
                let path = self
                    .state
                    .current_title
                    .as_ref()
                    .map_or(format!("{timestamp}"), |title| {
                        format!("{timestamp}-{title}")
                    });

                let path = format!("{path}-dump.json");

                let content = serde_json::to_string_pretty(&conversation)?;
                tokio::fs::write(path.as_str(), content).await?;

                CONSOLE.writeln(
                    TitleFormat::success("dump")
                        .sub_title(format!("path: {path}"))
                        .format(),
                )?;
            } else {
                CONSOLE.writeln(
                    TitleFormat::failed("dump")
                        .error("conversation not found")
                        .sub_title(format!("conversation_id: {conversation_id}"))
                        .format(),
                )?;
            }
        }
        Ok(())
    }

    fn handle_chat_response(&mut self, message: AgentMessage<ChatResponse>) -> Result<()> {
        match message.message {
            ChatResponse::Text(text) => {
                // Any agent that ends with "worker" is considered a worker agent.
                // Worker agents don't print anything to the console.
                if !message.agent.as_str().to_lowercase().ends_with("worker") {
                    CONSOLE.write(&text)?;
                }
            }
            ChatResponse::ToolCallStart(_) => {
                CONSOLE.newline()?;
                CONSOLE.newline()?;
            }
            ChatResponse::ToolCallEnd(tool_result) => {
                if !self.cli.verbose {
                    return Ok(());
                }

                let tool_name = tool_result.name.as_str();

                CONSOLE.writeln(format!("{}", tool_result.content.dimmed()))?;

                if tool_result.is_error {
                    CONSOLE.writeln(TitleFormat::failed(tool_name).format())?;
                } else {
                    CONSOLE.writeln(TitleFormat::success(tool_name).format())?;
                }
            }
            ChatResponse::Custom(event) => {
                if event.name == EVENT_TITLE {
                    self.state.current_title = Some(event.value);
                }
            }
            ChatResponse::Usage(u) => {
                self.state.usage = u;
            }
        }
        Ok(())
    } // Handle help chat in HELP mode
    async fn help_chat(&mut self, content: String) -> Result<()> {
        let conversation_id = self.init_conversation().await?;

        // Create a help query event
        let event = Self::create_user_help_query_event(content.clone());

        // Create the chat request with the help query event
        let chat = ChatRequest::new(event, conversation_id);

        tokio::spawn(TRACKER.dispatch(EventKind::Prompt(content)));

        match self.api.chat(chat).await {
            Ok(mut stream) => self.handle_chat_stream(&mut stream).await,
            Err(err) => Err(err),
        }
    }
}
