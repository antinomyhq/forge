use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_api::{
    AgentMessage, ChatRequest, ChatResponse, Conversation, ConversationId, Event, Mode, Model,
    ModelId, Workflow, API,
};
use forge_display::{MarkdownFormat, TitleFormat};
use forge_fs::ForgeFS;
use forge_spinner::SpinnerManager;
use forge_tracker::ToolCallPayload;
use inquire::error::InquireError;
use inquire::ui::{RenderConfig, Styled};
use inquire::Select;
use merge::Merge;
use serde::Deserialize;
use serde_json::Value;
use tokio_stream::StreamExt;

use crate::cli::Cli;
use crate::info::Info;
use crate::input::Console;
use crate::model::{Command, ForgeCommandManager};
use crate::state::UIState;
use crate::update::on_update;
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
    /// Writes a line to the console output
    /// Takes anything that implements ToString trait
    fn writeln<T: ToString>(&mut self, content: T) -> anyhow::Result<()> {
        self.spinner.write_ln(content)
    }
    /// Retrieve available models, using cache if present
    async fn get_models(&mut self) -> Result<Vec<Model>> {
        if let Some(models) = &self.state.cached_models {
            Ok(models.clone())
        } else {
            self.spinner.start(Some("Loading Models"))?;
            let models = self.api.models().await?;
            self.spinner.stop(None)?;
            self.state.cached_models = Some(models.clone());
            Ok(models)
        }
    }

    // Handle creating a new conversation
    async fn on_new(&mut self) -> Result<()> {
        self.state = UIState::default();
        self.init_conversation().await?;
        banner::display()?;

        Ok(())
    }

    // Set the current mode and update conversation variable
    async fn on_mode_change(&mut self, mode: Mode) -> Result<()> {
        // Set the mode variable in the conversation if a conversation exists
        let conversation_id = self.init_conversation().await?;

        // Override the mode that was reset by the conversation
        self.state.mode = mode.clone();

        // Retrieve the conversation, update it, and save it back
        if let Some(mut conversation) = self.api.conversation(&conversation_id).await? {
            conversation.set_variable("mode".to_string(), Value::from(mode.to_string()));
            self.api.upsert_conversation(conversation).await?;
        }

        // Update the workflow with the new mode
        self.api
            .update_workflow(self.cli.workflow.as_deref(), |workflow| {
                workflow
                    .variables
                    .insert("mode".to_string(), Value::from(mode.to_string()));
            })
            .await?;

        self.writeln(TitleFormat::action(format!(
            "Switched to '{}' mode",
            self.state.mode
        )))?;

        Ok(())
    }
    // Helper functions for creating events with the specific event names
    fn create_task_init_event<V: Into<Value>>(&self, content: V) -> Event {
        Event::new(EVENT_USER_TASK_INIT.to_string(), content)
    }

    fn create_task_update_event<V: Into<Value>>(&self, content: V) -> Event {
        Event::new(EVENT_USER_TASK_UPDATE.to_string(), content)
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

    pub async fn run(&mut self) {
        match self.run_inner().await {
            Ok(_) => {}
            Err(error) => {
                self.writeln(TitleFormat::error(format!("{error:?}")))
                    .unwrap();
            }
        }
    }

    async fn run_inner(&mut self) -> Result<()> {
        // Check for dispatch flag first
        if let Some(dispatch_json) = self.cli.event.clone() {
            return self.handle_dispatch(dispatch_json).await;
        }

        // Handle direct prompt if provided
        let prompt = self.cli.prompt.clone();
        if let Some(prompt) = prompt {
            self.on_message(prompt).await?;
            return Ok(());
        }

        // Display the banner in dimmed colors since we're in interactive mode
        banner::display()?;
        self.init_conversation().await?;

        // Get initial input from file or prompt
        let mut command = match &self.cli.command {
            Some(path) => self.console.upload(path).await?,
            None => self.prompt().await?,
        };

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                result = self.on_command(command) => {
                    match result {
                        Ok(exit) => if exit {return Ok(())},
                        Err(error) => {
                            tokio::spawn(
                                TRACKER.dispatch(forge_tracker::EventKind::Error(format!("{error:?}"))),
                            );
                            self.writeln(TitleFormat::error(format!("{error:?}")))?;
                        },
                    }
                }
            }

            self.spinner.stop(None)?;

            // Centralized prompt call at the end of the loop
            command = self.prompt().await?;
        }
    }

    async fn on_command(&mut self, command: Command) -> anyhow::Result<bool> {
        match command {
            Command::Compact => {
                self.on_compaction().await?;
            }
            Command::Dump(format) => {
                self.on_dump(format).await?;
            }
            Command::New => {
                self.on_new().await?;
            }
            Command::Info => {
                let info = Info::from(&self.state).extend(Info::from(&self.api.environment()));
                self.writeln(info)?;
            }
            Command::Message(ref content) => {
                self.on_message(content.clone()).await?;
            }
            Command::Act => {
                self.on_mode_change(Mode::Act).await?;
            }
            Command::Plan => {
                self.on_mode_change(Mode::Plan).await?;
            }
            Command::Help => {
                let info = Info::from(self.command.as_ref());
                self.writeln(info)?;
            }
            Command::Tools => {
                use crate::tools_display::format_tools;
                let tools = self.api.tools().await;
                let output = format_tools(&tools);
                self.writeln(output)?;
            }
            Command::Update => {
                on_update(self.api.clone(), None).await;
            }
            Command::Exit => {
                return Ok(true);
            }

            Command::Custom(event) => {
                self.on_custom_event(event.into()).await?;
            }
            Command::Model => {
                self.on_model_selection().await?;
            }
            Command::Shell(ref command) => {
                let mut s = command.split_whitespace();
                if let Some(command) = s.next() {
                    let args = s.collect::<Vec<_>>();
                    self.api.execute_shell_command_raw(command, &args).await?;
                } else {
                    return Err(anyhow::anyhow!(
                        "Correct Usage: ! <command> [args...] (e.g., !ls -la)"
                    ))
                    .context("Empty shell command.");
                }
            }
        }

        Ok(false)
    }

    async fn on_compaction(&mut self) -> Result<(), anyhow::Error> {
        self.spinner.start(Some("Compacting"))?;
        let conversation_id = self.init_conversation().await?;
        let compaction_result = self.api.compact_conversation(&conversation_id).await?;
        let token_reduction = compaction_result.token_reduction_percentage();
        let message_reduction = compaction_result.message_reduction_percentage();
        let content = TitleFormat::action(format!("Context size reduced by {token_reduction:.1}% (tokens), {message_reduction:.1}% (messages)"));
        self.writeln(content)?;
        Ok(())
    }

    /// Select a model from the available models
    /// Returns Some(ModelId) if a model was selected, or None if selection was
    /// canceled
    async fn select_model(&mut self) -> Result<Option<ModelId>> {
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
        match Select::new("Select a model:", model_ids)
            .with_help_message(
                "Type a model name or use arrow keys to navigate and Enter to select",
            )
            .with_render_config(render_config)
            .with_starting_cursor(starting_cursor)
            .prompt()
        {
            Ok(model) => Ok(Some(model)),
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                // Return None if selection was canceled
                Ok(None)
            }
            Err(err) => Err(err.into()),
        }
    }

    // Helper method to handle model selection and update the conversation
    async fn on_model_selection(&mut self) -> Result<()> {
        // Select a model
        let model_option = self.select_model().await?;

        // If no model was selected (user canceled), return early
        let model = match model_option {
            Some(model) => model,
            None => return Ok(()),
        };

        self.api
            .update_workflow(self.cli.workflow.as_deref(), |workflow| {
                workflow.model = Some(model.clone());
            })
            .await?;

        // Get the conversation to update
        let conversation_id = self.init_conversation().await?;

        if let Some(mut conversation) = self.api.conversation(&conversation_id).await? {
            // Update the model in the conversation
            conversation.set_main_model(model.clone())?;

            // Upsert the updated conversation
            self.api.upsert_conversation(conversation).await?;

            // Update the UI state with the new model
            self.update_model(model.clone());

            self.writeln(TitleFormat::action(format!("Switched to model: {model}")))?;
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
        let chat = ChatRequest::new(event.into(), conversation_id, self.state.mode.clone());

        // Process the event
        let mut stream = self.api.chat(chat).await?;
        self.handle_chat_stream(&mut stream).await
    }

    async fn init_conversation(&mut self) -> Result<ConversationId> {
        match self.state.conversation_id {
            Some(ref id) => Ok(id.clone()),
            None => {
                // Select a model if workflow doesn't have one
                let mut workflow = self.api.read_workflow(self.cli.workflow.as_deref()).await?;
                if workflow.model.is_none() {
                    workflow.model = Some(
                        self.select_model()
                            .await?
                            .ok_or(anyhow::anyhow!("Model selection is required to continue"))?,
                    );
                }

                // Perform update using the configuration
                let mut base_workflow = Workflow::default();
                base_workflow.merge(workflow.clone());

                on_update(self.api.clone(), base_workflow.updates.as_ref()).await;

                self.api
                    .write_workflow(self.cli.workflow.as_deref(), &workflow)
                    .await?;

                // Get the mode from the config
                let mode = workflow
                    .variables
                    .get("mode")
                    .and_then(|value| value.as_str().and_then(|m| Mode::from_str(m).ok()))
                    .unwrap_or(Mode::Act);

                self.state = UIState::new(mode).provider(self.api.environment().provider);
                self.command.register_all(&workflow);

                // We need to try and get the conversation ID first before fetching the model
                if let Some(ref path) = self.cli.conversation {
                    let conversation: Conversation = serde_json::from_str(
                        ForgeFS::read_to_string(path.as_os_str()).await?.as_str(),
                    )
                    .context("Failed to parse Conversation")?;

                    let conversation_id = conversation.id.clone();
                    self.state.conversation_id = Some(conversation_id.clone());
                    self.update_model(conversation.main_model()?);
                    self.api.upsert_conversation(conversation).await?;
                    Ok(conversation_id)
                } else {
                    let conversation = self.api.init_conversation(workflow).await?;
                    self.state.conversation_id = Some(conversation.id.clone());
                    self.update_model(conversation.main_model()?);
                    Ok(conversation.id)
                }
            }
        }
    }

    async fn on_message(&mut self, content: String) -> Result<()> {
        self.spinner.start(None)?;
        let conversation_id = self.init_conversation().await?;

        // Create a ChatRequest with the appropriate event type
        let event = if self.state.is_first {
            self.state.is_first = false;
            self.create_task_init_event(content.clone())
        } else {
            self.create_task_update_event(content.clone())
        };

        // Create the chat request with the event
        let chat = ChatRequest::new(event, conversation_id, self.state.mode.clone());

        match self.api.chat(chat).await {
            Ok(mut stream) => self.handle_chat_stream(&mut stream).await,
            Err(err) => Err(err),
        }
    }

    async fn handle_chat_stream(
        &mut self,
        stream: &mut (impl StreamExt<Item = Result<AgentMessage<ChatResponse>>> + Unpin),
    ) -> Result<()> {
        while let Some(message) = stream.next().await {
            match message {
                Ok(message) => self.handle_chat_response(message)?,
                Err(err) => {
                    self.spinner.stop(None)?;
                    return Err(err);
                }
            }
        }

        self.spinner.stop(None)?;

        Ok(())
    }

    /// Modified version of handle_dump that supports HTML format
    async fn on_dump(&mut self, format: Option<String>) -> Result<()> {
        if let Some(conversation_id) = self.state.conversation_id.clone() {
            let conversation = self.api.conversation(&conversation_id).await?;
            if let Some(conversation) = conversation {
                let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");

                if let Some(format) = format {
                    if format == "html" {
                        // Export as HTML
                        let html_content = conversation.to_html();
                        let path = format!("{timestamp}-dump.html");
                        tokio::fs::write(path.as_str(), html_content).await?;

                        self.writeln(
                            TitleFormat::action("Conversation HTML dump created".to_string())
                                .sub_title(path.to_string()),
                        )?;
                        return Ok(());
                    }
                } else {
                    // Default: Export as JSON
                    let path = format!("{timestamp}-dump.json");
                    let content = serde_json::to_string_pretty(&conversation)?;
                    tokio::fs::write(path.as_str(), content).await?;

                    self.writeln(
                        TitleFormat::action("Conversation JSON dump created".to_string())
                            .sub_title(path.to_string()),
                    )?;
                }
            } else {
                return Err(anyhow::anyhow!("Could not create dump"))
                    .context(format!("Conversation: {conversation_id} was not found"));
            }
        }
        Ok(())
    }

    fn handle_chat_response(&mut self, message: AgentMessage<ChatResponse>) -> Result<()> {
        match message.message {
            ChatResponse::Text { mut text, is_complete, is_md, is_summary } => {
                if is_complete && !text.trim().is_empty() {
                    if is_md || is_summary {
                        text = self.markdown.render(&text);
                    }

                    self.writeln(text)?;
                }
            }
            ChatResponse::ToolCallStart(_) => {
                self.spinner.stop(None)?;
            }
            ChatResponse::ToolCallEnd(toolcall_result) => {
                // Only track toolcall name in case of success else track the error.
                let payload = if toolcall_result.is_error {
                    ToolCallPayload::new(toolcall_result.name.into_string())
                        .with_cause(toolcall_result.content)
                } else {
                    ToolCallPayload::new(toolcall_result.name.into_string())
                };
                tokio::spawn(TRACKER.dispatch(forge_tracker::EventKind::ToolCall(payload)));

                self.spinner.start(None)?;
                if !self.cli.verbose {
                    return Ok(());
                }
            }
            ChatResponse::Usage(usage) => {
                self.state.usage = usage;
            }
        }
        Ok(())
    }

    fn update_model(&mut self, model: ModelId) {
        tokio::spawn(TRACKER.set_model(model.to_string()));
        self.state.model = Some(model);
    }

    async fn on_custom_event(&mut self, event: Event) -> Result<()> {
        let conversation_id = self.init_conversation().await?;
        let chat = ChatRequest::new(event, conversation_id, self.state.mode.clone());
        match self.api.chat(chat).await {
            Ok(mut stream) => self.handle_chat_stream(&mut stream).await,
            Err(err) => Err(err),
        }
    }
}
