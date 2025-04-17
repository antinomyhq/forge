use std::sync::Arc;

use anyhow::{Context, Result};
use colored::Colorize;
use forge_api::{
    AgentMessage, ChatRequest, ChatResponse, Conversation, ConversationId, Event, ModelId, API,
};
use forge_display::TitleFormat;
use forge_fs::ForgeFS;
use inquire::error::InquireError;
use inquire::ui::{RenderConfig, Styled};
use inquire::Select;
use serde::Deserialize;
use serde_json::Value;
use tokio_stream::StreamExt;
use tracing::error;

use crate::auto_update::update_forge;
use crate::cli::Cli;
use crate::console::CONSOLE;
use crate::info::Info;
use crate::input::Console;
use crate::model::{Command, ForgeCommandManager};
use crate::state::{Mode, UIState};
use crate::{banner, TRACKER};

// Event type constants moved to UI layer
pub const EVENT_USER_TASK_INIT: &str = "user_task_init";
pub const EVENT_USER_TASK_UPDATE: &str = "user_task_update";
pub const EVENT_USER_HELP_QUERY: &str = "user_help_query";
pub const EVENT_TITLE: &str = "title";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct PartialEvent {
    pub name: String,
    pub value: Value,
}

impl PartialEvent {
    pub fn new<V: Into<Value>>(name: impl ToString, value: V) -> Self {
        Self { name: name.to_string(), value: value.into() }
    }
}

impl From<PartialEvent> for Event {
    fn from(value: PartialEvent) -> Self {
        Event::new(value.name, value.value)
    }
}

pub struct UI<F> {
    state: UIState,
    api: Arc<F>,
    console: Console,
    command: Arc<ForgeCommandManager>,
    cli: Cli,
    #[allow(dead_code)] // The guard is kept alive by being held in the struct
    _guard: forge_tracker::Guard,
}

impl<F: API> UI<F> {
    // Set the current mode and update conversation variable
    async fn handle_mode_change(&mut self, mode: Mode) -> Result<()> {
        // Set the mode variable in the conversation if a conversation exists
        let conversation_id = self.init_conversation().await?;

        // Override the mode that was reset by the conversation
        self.state.mode = mode.clone();

        self.api
            .set_variable(
                &conversation_id,
                "mode".to_string(),
                Value::from(mode.to_string()),
            )
            .await?;

        // Print a mode-specific message
        let mode_message = match self.state.mode {
            Mode::Act => "mode - executes commands and makes file changes",
            Mode::Plan => "mode - plans actions without making changes",
            Mode::Help => "mode - answers questions (type /act or /plan to switch back)",
        };

        CONSOLE.write(
            TitleFormat::success(mode.to_string())
                .sub_title(mode_message)
                .format(),
        )?;

        Ok(())
    }
    // Helper functions for creating events with the specific event names
    fn create_task_init_event<V: Into<Value>>(content: V) -> Event {
        Event::new(EVENT_USER_TASK_INIT, content)
    }

    fn create_task_update_event<V: Into<Value>>(content: V) -> Event {
        Event::new(EVENT_USER_TASK_UPDATE, content)
    }
    fn create_user_help_query_event<V: Into<Value>>(content: V) -> Event {
        Event::new(EVENT_USER_HELP_QUERY, content)
    }

    pub fn init(cli: Cli, api: Arc<F>) -> Result<Self> {
        // Parse CLI arguments first to get flags
        let env = api.environment();
        let command = Arc::new(ForgeCommandManager::default());
        Ok(Self {
            state: Default::default(),
            api,
            console: Console::new(env.clone(), command.clone()),
            cli,
            command,
            _guard: forge_tracker::init_tracing(env.log_path())?,
        })
    }

    async fn prompt(&self) -> Result<Command> {
        // Prompt the user for input
        self.console.prompt(Some(self.state.clone().into())).await
    }

    pub async fn run(&mut self) -> Result<()> {
        // Check for dispatch flag first
        if let Some(dispatch_json) = self.cli.event.clone() {
            return self.handle_dispatch(dispatch_json).await;
        }

        // Handle direct prompt if provided
        let prompt = self.cli.prompt.clone();
        if let Some(prompt) = prompt {
            self.chat(prompt).await?;
            return Ok(());
        }

        // Display the banner in dimmed colors since we're in interactive mode
        self.init_conversation().await?;
        banner::display(self.command.command_names())?;

        // Get initial input from file or prompt
        let mut input = match &self.cli.command {
            Some(path) => self.console.upload(path).await?,
            None => self.prompt().await?,
        };

        loop {
            match input {
                Command::Dump => {
                    self.handle_dump().await?;
                    input = self.prompt().await?;
                    continue;
                }
                Command::New => {
                    self.state = UIState::default();
                    self.init_conversation().await?;
                    banner::display(self.command.command_names())?;
                    input = self.prompt().await?;

                    continue;
                }
                Command::Info => {
                    let info =
                        Info::from(&self.api.environment()).extend(Info::from(&self.state.usage));

                    CONSOLE.writeln(info.to_string())?;

                    input = self.prompt().await?;
                    continue;
                }
                Command::Message(ref content) => {
                    let chat_result = match self.state.mode {
                        Mode::Help => {
                            self.dispatch_event(Self::create_user_help_query_event(content.clone()))
                                .await
                        }
                        _ => self.chat(content.clone()).await,
                    };
                    if let Err(err) = chat_result {
                        tokio::spawn(
                            TRACKER.dispatch(forge_tracker::EventKind::Error(format!("{:?}", err))),
                        );
                        error!(error = ?err, "Chat request failed");

                        CONSOLE.writeln(TitleFormat::failed(format!("{:?}", err)).format())?;
                    }

                    input = self.prompt().await?;
                }
                Command::Act => {
                    self.handle_mode_change(Mode::Act).await?;

                    input = self.prompt().await?;
                    continue;
                }
                Command::Plan => {
                    self.handle_mode_change(Mode::Plan).await?;
                    input = self.prompt().await?;
                    continue;
                }
                Command::Help => {
                    self.handle_mode_change(Mode::Help).await?;

                    input = self.prompt().await?;
                    continue;
                }
                Command::Exit => {
                    CONSOLE.writeln(
                        TitleFormat::execute("exit")
                            .sub_title("initializing graceful shutdown... thank you!")
                            .format(),
                    )?;

                    update_forge().await;

                    break;
                }

                Command::Custom(event) => {
                    if let Err(e) = self.dispatch_event(event.into()).await {
                        CONSOLE.writeln(
                            TitleFormat::failed("Failed to execute the command.")
                                .sub_title("Command Execution")
                                .error(e.to_string())
                                .format(),
                        )?;
                    }

                    input = self.prompt().await?;
                }
                Command::Model => {
                    self.handle_model_selection().await?;
                    input = self.prompt().await?;
                    continue;
                }
            }
        }

        Ok(())
    }

    // Helper method to handle model selection and update the conversation
    async fn handle_model_selection(&mut self) -> Result<()> {
        // Fetch available models
        let models = self.api.models().await?;

        // Create list of model IDs for selection
        let model_ids: Vec<ModelId> = models.into_iter().map(|m| m.id).collect();

        // Create a custom render config with the specified icons
        let render_config = RenderConfig::default()
            .with_scroll_up_prefix(Styled::new("⇡"))
            .with_scroll_down_prefix(Styled::new("⇣"))
            .with_highlighted_option_prefix(Styled::new("➤"));

        // Find the index of the current model
        let starting_cursor = self
            .state
            .model
            .as_ref()
            .and_then(|current| model_ids.iter().position(|id| id == current))
            .unwrap_or(0);

        // Use inquire to select a model, with the current model pre-selected
        let model = match Select::new("Select a model:", model_ids)
            .with_help_message("Use arrow keys to navigate and Enter to select")
            .with_render_config(render_config)
            .with_starting_cursor(starting_cursor)
            .prompt()
        {
            Ok(model) => model,
            Err(InquireError::OperationCanceled) => return Ok(()),
            Err(err) => return Err(err.into()),
        };

        // Get the conversation to update
        let conversation_id = self.init_conversation().await?;

        if let Some(mut conversation) = self.api.conversation(&conversation_id).await? {
            // Update the model in the conversation
            conversation.set_main_model(model.clone())?;

            // Upsert the updated conversation
            self.api.upsert_conversation(conversation).await?;

            // Update the UI state with the new model
            self.state.model = Some(model.clone());

            CONSOLE.writeln(
                TitleFormat::success("model")
                    .sub_title(format!("switched to: {}", model))
                    .format(),
            )?;
        } else {
            CONSOLE.writeln(
                TitleFormat::failed("model")
                    .error("Failed to update model: conversation not found")
                    .format(),
            )?;
        }

        Ok(())
    }

    // Handle dispatching events from the CLI
    async fn handle_dispatch(&mut self, json: String) -> Result<()> {
        // Initialize the conversation
        let conversation_id = self.init_conversation().await?;

        // Parse the JSON to determine the event name and value
        let event: PartialEvent = serde_json::from_str(&json)?;

        // Create the chat request with the event
        let chat = ChatRequest::new(event.into(), conversation_id);

        // Process the event
        let mut stream = self.api.chat(chat).await?;
        self.handle_chat_stream(&mut stream).await
    }

    async fn init_conversation(&mut self) -> Result<ConversationId> {
        match self.state.conversation_id {
            Some(ref id) => Ok(id.clone()),
            None => {
                let config = self.api.load(self.cli.workflow.as_deref()).await?;

                // Get the mode from the config
                let mode = config
                    .variables
                    .get("mode")
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or(Mode::Act);

                self.state = UIState::new(mode);
                self.command.register_all(&config);

                // We need to try and get the conversation ID first before fetching the model
                if let Some(ref path) = self.cli.conversation {
                    let conversation: Conversation = serde_json::from_str(
                        ForgeFS::read_to_string(path.as_os_str()).await?.as_str(),
                    )
                    .context("Failed to parse Conversation")?;

                    let conversation_id = conversation.id.clone();
                    self.state.model = Some(conversation.main_model()?);
                    self.state.conversation_id = Some(conversation_id.clone());
                    self.api.upsert_conversation(conversation).await?;
                    Ok(conversation_id)
                } else {
                    let conversation = self.api.init(config.clone()).await?;
                    self.state.model = Some(conversation.main_model()?);
                    self.state.conversation_id = Some(conversation.id.clone());
                    Ok(conversation.id)
                }
            }
        }
    }

    async fn chat(&mut self, content: String) -> Result<()> {
        let conversation_id = self.init_conversation().await?;

        // Create a ChatRequest with the appropriate event type
        let event = if self.state.is_first {
            self.state.is_first = false;
            Self::create_task_init_event(content.clone())
        } else {
            Self::create_task_update_event(content.clone())
        };

        // Create the chat request with the event
        let chat = ChatRequest::new(event, conversation_id);

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
            ChatResponse::Text { text: content, is_complete } => {
                if is_complete {
                    CONSOLE.writeln(content.trim())?;
                } else {
                    CONSOLE.write(content.dimmed().to_string())?;
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
            ChatResponse::Event(event) => {
                if event.name == EVENT_TITLE {
                    self.state.current_title =
                        Some(event.value.as_str().unwrap_or_default().to_string());
                }
            }
            ChatResponse::Usage(u) => {
                self.state.usage = u;
            }
        }
        Ok(())
    }

    async fn dispatch_event(&mut self, event: Event) -> Result<()> {
        let conversation_id = self.init_conversation().await?;
        let chat = ChatRequest::new(event, conversation_id);
        match self.api.chat(chat).await {
            Ok(mut stream) => self.handle_chat_stream(&mut stream).await,
            Err(err) => Err(err),
        }
    }
}
