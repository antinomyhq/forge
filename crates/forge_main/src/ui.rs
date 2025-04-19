use std::sync::Arc;

use anyhow::{Context, Result};
use forge_api::{
    AgentMessage, ChatRequest, ChatResponse, Conversation, ConversationId, Event, Model, ModelId,
    API,
};
use forge_display::{MarkdownFormat, TitleFormat};
use forge_fs::ForgeFS;
use forge_spinner::SpinnerManager;
use inquire::error::InquireError;
use inquire::ui::{RenderConfig, Styled};
use inquire::Select;
use serde::Deserialize;
use serde_json::Value;
use tokio_stream::StreamExt;
use tracing::error;

use crate::auto_update::update_forge;
use crate::cli::Cli;
use crate::info::Info;
use crate::input::Console;
use crate::model::{Command, ForgeCommandManager};
use crate::state::{Mode, UIState};
use crate::{banner, TRACKER};

// Event type constants moved to UI layer
pub const EVENT_USER_TASK_INIT: &str = "user_task_init";
pub const EVENT_USER_TASK_UPDATE: &str = "user_task_update";

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
    markdown: MarkdownFormat,
    state: UIState,
    api: Arc<F>,
    console: Console,
    command: Arc<ForgeCommandManager>,
    cli: Cli,
    spinner: SpinnerManager,
    #[allow(dead_code)] // The guard is kept alive by being held in the struct
    _guard: forge_tracker::Guard,
}

impl<F: API> UI<F> {
    /// Retrieve available models, using cache if present
    async fn get_models(&mut self) -> Result<Vec<Model>> {
        if let Some(models) = &self.state.cached_models {
            Ok(models.clone())
        } else {
            let models = self.api.models().await?;
            self.state.cached_models = Some(models.clone());
            Ok(models)
        }
    }

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
            Mode::Act => "Switched to 'ACT' mode",
            Mode::Plan => "Switched to 'PLAN' mode",
        };

        println!("{}", TitleFormat::action(mode_message).format());

        Ok(())
    }
    // Helper functions for creating events with the specific event names
    fn create_task_init_event<V: Into<Value>>(content: V) -> Event {
        Event::new(EVENT_USER_TASK_INIT, content)
    }

    fn create_task_update_event<V: Into<Value>>(content: V) -> Event {
        Event::new(EVENT_USER_TASK_UPDATE, content)
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
            spinner: SpinnerManager::new(),
            markdown: MarkdownFormat::new(),
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
        banner::display()?;

        // Get initial input from file or prompt
        let mut input = match &self.cli.command {
            Some(path) => self.console.upload(path).await?,
            None => self.prompt().await?,
        };

        loop {
            match input {
                Command::Compact => {
                    self.spinner.start()?;
                    let conversation_id = self.init_conversation().await?;
                    let compaction_result = self.api.compact_conversation(&conversation_id).await?;

                    // Calculate percentage reduction
                    let token_reduction = compaction_result.token_reduction_percentage();
                    let message_reduction = compaction_result.message_reduction_percentage();

                    let content = TitleFormat::action(format!(
                        "Context size reduced by {:.1}% (tokens), {:.1}% (messages)",
                        token_reduction, message_reduction
                    ))
                    .format();
                    self.spinner.stop(Some(content))?;
                }
                Command::Dump => {
                    self.handle_dump().await?;
                }
                Command::New => {
                    self.state = UIState::default();
                    self.init_conversation().await?;
                    banner::display()?;
                }
                Command::Info => {
                    let info = Info::from(&self.state).extend(Info::from(&self.api.environment()));
                    println!("{}", info);
                }
                Command::Message(ref content) => {
                    self.spinner.start()?;
                    let chat_result = self.chat(content.clone()).await;
                    if let Err(err) = chat_result {
                        tokio::spawn(
                            TRACKER.dispatch(forge_tracker::EventKind::Error(format!("{:?}", err))),
                        );
                        error!(error = ?err, "Chat request failed");

                        println!(
                            "{}",
                            TitleFormat::action("error")
                                .error(format!("{:?}", err))
                                .format()
                        );
                    }
                }
                Command::Act => {
                    self.handle_mode_change(Mode::Act).await?;
                }
                Command::Plan => {
                    self.handle_mode_change(Mode::Plan).await?;
                }
                Command::Help => {
                    let info = Info::from(self.command.as_ref());
                    println!("{}", info);
                }
                Command::Exit => {
                    update_forge().await;

                    break;
                }

                Command::Custom(event) => {
                    if let Err(e) = self.dispatch_event(event.into()).await {
                        println!(
                            "{}",
                            TitleFormat::action("Failed to execute the command")
                                .sub_title("Command Execution")
                                .error(e.to_string())
                                .format()
                        );
                    }
                }
                Command::Model => {
                    self.handle_model_selection().await?;
                }
                Command::Shell(ref command) => {
                    // Execute the shell command using the existing infrastructure
                    // Get the working directory from the environment service instead of std::env
                    let cwd = self.api.environment().cwd;

                    // Execute the command
                    let _ = self.api.execute_shell_command(command, cwd).await;
                }
            }

            // Centralized prompt call at the end of the loop
            input = self.prompt().await?;
        }

        Ok(())
    }

    // Helper method to handle model selection and update the conversation
    async fn handle_model_selection(&mut self) -> Result<()> {
        // Fetch available models
        let models = self.get_models().await?;

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
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                return Ok(())
            }
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

            println!(
                "{}",
                TitleFormat::action(format!("Switched to model: {}", model)).format()
            );
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
        // Set up a tokio interval to update the spinner every second
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    self.spinner.stop(None)?;
                    return Ok(());
                }
                _ = interval.tick() => {
                    // Update the spinner with elapsed time
                    if let Err(e) = self.spinner.update_time() {
                        tracing::warn!("Failed to update spinner time: {}", e);
                    }
                }
                maybe_message = stream.next() => {
                    match maybe_message {
                        Some(Ok(message)) => self.handle_chat_response(message)?,
                        Some(Err(err)) => {
                            self.spinner.stop(None)?;
                            return Err(err);
                        }
                        None => {
                            self.spinner.stop(None)?;
                            return Ok(())
                        },
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
                let path = format!("{timestamp}-dump.json");

                let content = serde_json::to_string_pretty(&conversation)?;
                tokio::fs::write(path.as_str(), content).await?;

                println!(
                    "{}",
                    TitleFormat::action("Conversation dump created".to_string())
                        .sub_title(path.to_string())
                        .format()
                );
            } else {
                println!(
                    "{}",
                    TitleFormat::action("Could not create dump")
                        .error("Conversation not found")
                        .sub_title(format!("conversation_id: {conversation_id}"))
                        .format()
                );
            }
        }
        Ok(())
    }

    fn handle_chat_response(&mut self, message: AgentMessage<ChatResponse>) -> Result<()> {
        match message.message {
            ChatResponse::Text { mut text, is_complete, is_md } => {
                if is_complete && !text.trim().is_empty() {
                    if is_md {
                        text = self.markdown.render(&text);
                    }
                    self.spinner.stop(Some(text))?;
                }
            }
            ChatResponse::ToolCallStart(_) => {
                self.spinner.stop(None)?;
            }
            ChatResponse::ToolCallEnd(_) => {
                self.spinner.start()?;
                if !self.cli.verbose {
                    return Ok(());
                }
            }
            ChatResponse::Event(_) => {
                // Event handling removed
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
