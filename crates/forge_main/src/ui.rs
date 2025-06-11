use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::Arc;

use anyhow::{Context, Result};
use colored::Colorize;
use convert_case::{Case, Casing};
use forge_api::{
    AgentId, ChatRequest, ChatResponse, Conversation, ConversationId, Event, Model, ModelId,
    Workflow, API,
};
use forge_display::{MarkdownFormat, TitleFormat};
use forge_domain::{McpConfig, McpServerConfig, Scope};
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

use crate::cli::{Cli, McpCommand, TopLevelCommand, Transport};
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

    /// Retrieve available models
    async fn get_models(&mut self) -> Result<Vec<Model>> {
        self.spinner.start(Some("Loading"))?;
        let models = self.api.models().await?;
        self.spinner.stop(None)?;
        Ok(models)
    }

    // Handle creating a new conversation
    async fn on_new(&mut self) -> Result<()> {
        self.init_state().await?;
        banner::display()?;

        Ok(())
    }

    async fn active_workflow(&self) -> Result<Workflow> {
        // Read the current workflow to validate the agent
        let workflow = self.api.read_workflow(self.cli.workflow.as_deref()).await?;
        let mut base_workflow = Workflow::default();
        base_workflow.merge(workflow.clone());
        Ok(base_workflow)
    }

    // Set the current mode and update conversation variable
    async fn on_agent_change(&mut self, agent_id: AgentId) -> Result<()> {
        let workflow = self.active_workflow().await?;

        // Convert string to AgentId for validation
        let agent = workflow.get_agent(&AgentId::new(agent_id))?;

        let conversation_id = self.init_conversation().await?;
        if let Some(mut conversation) = self.api.conversation(&conversation_id).await? {
            conversation.set_variable("operating_agent".into(), Value::from(agent.id.as_str()));
            self.api.upsert_conversation(conversation).await?;
        }

        // Reset is_first to true when switching agents
        self.state.is_first = true;
        self.state.operating_agent = agent.id.clone();

        // Update the workflow with the new operating agent.
        self.api
            .update_workflow(self.cli.workflow.as_deref(), |workflow| {
                workflow.variables.insert(
                    "operating_agent".to_string(),
                    Value::from(agent.id.as_str()),
                );
            })
            .await?;

        self.writeln(TitleFormat::action(format!(
            "Switched to agent {}",
            agent.id.as_str().to_case(Case::UpperSnake).bold()
        )))?;

        Ok(())
    }

    fn create_task_event<V: Into<Value>>(
        &self,
        content: V,
        event_name: &str,
    ) -> anyhow::Result<Event> {
        let operating_agent = &self.state.operating_agent;
        Ok(Event::new(
            format!("{operating_agent}/{event_name}"),
            content,
        ))
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
            _guard: forge_tracker::init_tracing(env.log_path(), TRACKER.clone())?,
        })
    }

    async fn prompt(&self) -> Result<Command> {
        // Prompt the user for input
        self.console.prompt(self.state.clone().into()).await
    }

    pub async fn run(&mut self) {
        match self.run_inner().await {
            Ok(_) => {}
            Err(error) => {
                eprintln!("{}", TitleFormat::error(format!("{error:?}")));
            }
        }
    }

    async fn run_inner(&mut self) -> Result<()> {
        if let Some(mcp) = self.cli.subcommands.clone() {
            return self.handle_subcommands(mcp).await;
        }

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
        self.init_state().await?;

        // Get initial input from file or prompt
        let mut command = match &self.cli.command {
            Some(path) => self.console.upload(path).await?,
            None => self.prompt().await?,
        };

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("User interrupted operation with Ctrl+C");
                }
                result = self.on_command(command) => {
                    match result {
                        Ok(exit) => if exit {return Ok(())},
                        Err(error) => {
                            if let Some(conversation_id) = self.state.conversation_id.as_ref() {
                                if let Some(conversation) = self.api.conversation(conversation_id).await.ok().flatten() {
                                    TRACKER.set_conversation(conversation).await;
                                }
                            }
                            tokio::spawn(
                                TRACKER.dispatch(forge_tracker::EventKind::Error(format!("{error:?}"))),
                            );
                            self.spinner.stop(None)?;
                            eprintln!("{}", TitleFormat::error(format!("{error:?}")));
                        },
                    }
                }
            }

            self.spinner.stop(None)?;

            // Centralized prompt call at the end of the loop
            command = self.prompt().await?;
        }
    }

    async fn handle_subcommands(&mut self, subcommand: TopLevelCommand) -> anyhow::Result<()> {
        match subcommand {
            TopLevelCommand::Mcp(mcp_command) => match mcp_command.command {
                McpCommand::Add(add) => {
                    let name = add.name;
                    let scope: Scope = add.scope.into();
                    // Create the appropriate server type based on transport
                    let server = match add.transport {
                        Transport::Stdio => McpServerConfig::new_stdio(
                            add.command_or_url.clone(),
                            add.args.clone(),
                            Some(parse_env(add.env.clone())),
                        ),
                        Transport::Sse => McpServerConfig::new_sse(add.command_or_url.clone()),
                    };
                    // Command/URL already set in the constructor

                    self.update_mcp_config(&scope, |config| {
                        config.mcp_servers.insert(name.to_string(), server);
                    })
                    .await?;

                    self.writeln(TitleFormat::info(format!("Added MCP server '{name}'")))?;
                }
                McpCommand::List => {
                    let mcp_servers = self.api.read_mcp_config().await?;
                    if mcp_servers.is_empty() {
                        self.writeln(TitleFormat::error("No MCP servers found"))?;
                    }

                    let mut output = String::new();
                    for (name, server) in mcp_servers.mcp_servers {
                        output.push_str(&format!("{name}: {server}"));
                    }
                    self.writeln(output)?;
                }
                McpCommand::Remove(rm) => {
                    let name = rm.name.clone();
                    let scope: Scope = rm.scope.into();

                    self.update_mcp_config(&scope, |config| {
                        config.mcp_servers.remove(name.as_str());
                    })
                    .await?;

                    self.writeln(TitleFormat::info(format!("Removed server: {name}")))?;
                }
                McpCommand::Get(val) => {
                    let name = val.name.clone();
                    let config = self.api.read_mcp_config().await?;
                    let server = config
                        .mcp_servers
                        .get(name.as_str())
                        .ok_or(anyhow::anyhow!("Server not found"))?;

                    let mut output = String::new();
                    output.push_str(&format!("{name}: {server}"));
                    self.writeln(TitleFormat::info(output))?;
                }
                McpCommand::AddJson(add_json) => {
                    let server = serde_json::from_str::<McpServerConfig>(add_json.json.as_str())
                        .context("Failed to parse JSON")?;
                    let scope: Scope = add_json.scope.into();
                    let name = add_json.name.clone();
                    self.update_mcp_config(&scope, |config| {
                        config.mcp_servers.insert(name.clone(), server);
                    })
                    .await?;

                    self.writeln(TitleFormat::info(format!(
                        "Added server: {}",
                        add_json.name
                    )))?;
                }
            },
        }
        Ok(())
    }

    async fn on_command(&mut self, command: Command) -> anyhow::Result<bool> {
        match command {
            Command::Compact => {
                self.spinner.start(Some("Compacting"))?;
                self.on_compaction().await?;
            }
            Command::Dump(format) => {
                self.spinner.start(Some("Dumping"))?;
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
                self.spinner.start(None)?;
                self.on_message(content.clone()).await?;
            }
            Command::Forge => {
                self.on_agent_change(AgentId::FORGE).await?;
            }
            Command::Muse => {
                self.on_agent_change(AgentId::MUSE).await?;
            }
            Command::Help => {
                let info = Info::from(self.command.as_ref());
                self.writeln(info)?;
            }
            Command::Tools => {
                self.spinner.start(Some("Loading"))?;
                use crate::tools_display::format_tools;
                let tools = self.api.tools().await?;

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
                self.spinner.start(None)?;
                self.on_custom_event(event.into()).await?;
            }
            Command::Model => {
                self.on_model_selection().await?;
            }
            Command::Shell(ref command) => {
                self.api.execute_shell_command_raw(command).await?;
            }
            Command::Agent => {
                // Read the current workflow to validate the agent
                let workflow = self.active_workflow().await?;

                #[derive(Clone)]
                struct Agent {
                    id: AgentId,
                    label: String,
                }

                impl Display for Agent {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "{}", self.label)
                    }
                }
                let n = workflow
                    .agents
                    .iter()
                    .map(|a| a.id.as_str().len())
                    .max()
                    .unwrap_or_default();
                let display_agents = workflow
                    .agents
                    .into_iter()
                    .map(|agent| {
                        if let Some(title) = &agent.title {
                            let label = format!(
                                "{:<n$} {}",
                                agent.id.as_str().to_case(Case::UpperSnake).bold(),
                                title.lines().collect::<Vec<_>>().join(" ").dimmed()
                            );
                            Agent { label, id: agent.id }
                        } else {
                            Agent { id: agent.id, label: "<Missing agent title>".to_string() }
                        }
                    })
                    .collect::<Vec<_>>();

                let select_prompt = inquire::Select::new(
                    "select the agent from following list",
                    display_agents.clone(),
                );
                if let Ok(selected_agent) = select_prompt.prompt() {
                    self.on_agent_change(selected_agent.id).await?;
                }
            }
        }

        Ok(false)
    }
    async fn on_compaction(&mut self) -> Result<(), anyhow::Error> {
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
        let models = self
            .get_models()
            .await?
            .into_iter()
            .map(CliModel)
            .collect::<Vec<_>>();

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
            .and_then(|current| models.iter().position(|m| &m.0.id == current))
            .unwrap_or(0);

        // Use inquire to select a model, with the current model pre-selected
        match Select::new("Select a model:", models)
            .with_help_message(
                "Type a model name or use arrow keys to navigate and Enter to select",
            )
            .with_render_config(render_config)
            .with_starting_cursor(starting_cursor)
            .prompt()
        {
            Ok(model) => Ok(Some(model.0.id)),
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
        let chat = ChatRequest::new(event.into(), conversation_id);

        self.on_chat(chat).await
    }

    async fn init_conversation(&mut self) -> Result<ConversationId> {
        match self.state.conversation_id {
            Some(ref id) => Ok(id.clone()),
            None => {
                self.spinner.start(Some("Initializing"))?;

                // Select a model if workflow doesn't have one
                let workflow = self.init_state().await?;
                // We need to try and get the conversation ID first before fetching the model
                let id = if let Some(ref path) = self.cli.conversation {
                    let conversation: Conversation = serde_json::from_str(
                        ForgeFS::read_to_string(path.as_os_str()).await?.as_str(),
                    )
                    .context("Failed to parse Conversation")?;

                    let conversation_id = conversation.id.clone();
                    self.state.conversation_id = Some(conversation_id.clone());
                    self.update_model(conversation.main_model()?);
                    self.api.upsert_conversation(conversation).await?;
                    conversation_id
                } else {
                    let conversation = self.api.init_conversation(workflow).await?;
                    self.state.conversation_id = Some(conversation.id.clone());
                    self.update_model(conversation.main_model()?);
                    conversation.id
                };

                Ok(id)
            }
        }
    }

    /// Initialize the state of the UI
    async fn init_state(&mut self) -> Result<Workflow> {
        let mut workflow = self.api.read_workflow(self.cli.workflow.as_deref()).await?;
        if workflow.model.is_none() {
            workflow.model = Some(
                self.select_model()
                    .await?
                    .ok_or(anyhow::anyhow!("Model selection is required to continue"))?,
            );
        }
        let mut base_workflow = Workflow::default();
        base_workflow.merge(workflow.clone());
        on_update(self.api.clone(), base_workflow.updates.as_ref()).await;
        self.api
            .write_workflow(self.cli.workflow.as_deref(), &workflow)
            .await?;

        self.command.register_all(&base_workflow);
        self.state = UIState::new(base_workflow).provider(self.api.environment().provider);

        Ok(workflow)
    }

    async fn on_message(&mut self, content: String) -> Result<()> {
        let conversation_id = self.init_conversation().await?;

        // Create a ChatRequest with the appropriate event type
        let event = if self.state.is_first {
            self.state.is_first = false;
            self.create_task_event(content, EVENT_USER_TASK_INIT)?
        } else {
            self.create_task_event(content, EVENT_USER_TASK_UPDATE)?
        };

        // Create the chat request with the event
        let chat = ChatRequest::new(event, conversation_id);

        self.on_chat(chat).await
    }

    async fn on_chat(&mut self, chat: ChatRequest) -> Result<()> {
        let mut stream = self.api.chat(chat).await?;

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

    fn handle_chat_response(&mut self, message: ChatResponse) -> Result<()> {
        match message {
            ChatResponse::Text { mut text, is_complete, is_md, is_summary } => {
                if is_complete && !text.trim().is_empty() {
                    if is_md || is_summary {
                        tracing::info!(message = %text, "Agent Response");
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
                let payload = if toolcall_result.is_error() {
                    let mut r = ToolCallPayload::new(toolcall_result.name.to_string());
                    if let Some(cause) = toolcall_result.output.as_str() {
                        r = r.with_cause(cause.to_string());
                    }
                    r
                } else {
                    ToolCallPayload::new(toolcall_result.name.to_string())
                };
                tokio::spawn(TRACKER.dispatch(forge_tracker::EventKind::ToolCall(payload)));

                self.spinner.start(None)?;
                if !self.cli.verbose {
                    return Ok(());
                }
            }
            ChatResponse::Usage(mut usage) => {
                // accumulate the cost
                usage.cost = usage
                    .cost
                    .map(|cost| cost + self.state.usage.cost.as_ref().map_or(0.0, |c| *c));
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
        let chat = ChatRequest::new(event, conversation_id);
        self.on_chat(chat).await
    }

    async fn update_mcp_config(&self, scope: &Scope, f: impl FnOnce(&mut McpConfig)) -> Result<()> {
        let mut config = self.api.read_mcp_config().await?;
        f(&mut config);
        self.api.write_mcp_config(scope, &config).await?;

        Ok(())
    }
}

fn parse_env(env: Vec<String>) -> BTreeMap<String, String> {
    env.into_iter()
        .filter_map(|s| {
            let mut parts = s.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                Some((key.to_string(), value.to_string()))
            } else {
                None
            }
        })
        .collect()
}

struct CliModel(Model);

impl Display for CliModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.id)?;
        if self.0.tools_supported.unwrap_or_default() {
            write!(f, " {}", "(tool_use)".dimmed())?;
        }
        Ok(())
    }
}
