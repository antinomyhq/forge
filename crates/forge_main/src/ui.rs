use std::sync::Arc;

use anyhow::Result;
use colored::Colorize;
use forge_api::{AgentMessage, Agent, ChatRequest, ChatResponse, ConversationId, Event, Model, API};
use forge_display::TitleFormat;
use lazy_static::lazy_static;
use serde::Deserialize;
use serde_json::Value;
use tokio_stream::StreamExt;
use tracing::error;

use crate::banner;
use crate::cli::Cli;
use crate::console::CONSOLE;
use crate::info::Info;
use crate::input::Console;
use crate::model::{Command, ForgeCommandManager, UserInput};
use crate::state::{Mode, UIState};

// Event type constants moved to UI layer
pub const EVENT_USER_TASK_INIT: &str = "user_task_init";
pub const EVENT_USER_TASK_UPDATE: &str = "user_task_update";
pub const EVENT_USER_HELP_QUERY: &str = "user_help_query";
pub const EVENT_TITLE: &str = "title";

lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct PartialEvent {
    pub name: String,
    pub value: String,
}

impl PartialEvent {
    pub fn new(name: impl ToString, value: impl ToString) -> Self {
        Self { name: name.to_string(), value: value.to_string() }
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
    
    // Format agent information into Info struct for display
    fn format_agent_info(agents: &[Agent]) -> Info {
        let mut info = Info::new().add_title("Agents");
        
        if agents.is_empty() {
            info = info.add_key_value("Status", "No agents found in active workflow");
        } else {
            // For each agent, add information
            for agent in agents {
                // Add agent ID as a title for each agent section
                info = info.add_title(agent.id.as_str());
                
                // Add model information if specified
                if let Some(model) = &agent.model {
                    info = info.add_key_value("Model", model.as_str());
                }
                
                // Add description if available
                if let Some(description) = &agent.description {
                    info = info.add_key_value("Description", description);
                }
                
                // Add tool_supported flag if specified
                if let Some(tool_supported) = agent.tool_supported {
                    info = info.add_key_value("Tools Supported", if tool_supported { "Yes" } else { "No" });
                }
                
                // Add tools if specified
                if let Some(tools) = &agent.tools {
                    if !tools.is_empty() {
                        let tool_list = tools.iter()
                            .map(|t| t.as_str())
                            .collect::<Vec<&str>>()
                            .join(", ");
                        info = info.add_key_value("Tools", tool_list);
                    }
                }
                
                // Add subscribed events if specified
                if let Some(events) = &agent.subscribe {
                    if !events.is_empty() {
                        let events_list = events.join(", ");
                        info = info.add_key_value("Events", events_list);
                    }
                }
                
                // Add max_turns if specified
                if let Some(max_turns) = agent.max_turns {
                    info = info.add_key_value("Max Turns", max_turns.to_string());
                }
            }
        }
        
        info
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
        let command = Arc::new(ForgeCommandManager::default());
        Ok(Self {
            state: Default::default(),
            api,
            console: Console::new(env.clone(), command.clone()),
            cli,
            command,
            models: None,
            _guard: forge_tracker::init_tracing(env.log_path())?,
        })
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
                    self.state = Default::default();
                    self.init_conversation().await?;
                    banner::display(self.command.command_names())?;
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
                Command::Agents => {
                    // Check if a conversation and workflow exist
                    if let Some(conversation_id) = self.state.conversation_id.clone() {
                        // Get the conversation (which contains workflow)
                        if let Some(conversation) = self.api.conversation(&conversation_id).await? {
                            // Check if workflow contains agents
                            if let Some(workflow) = &conversation.workflow {
                                let agents = &workflow.agents;
                                
                                // Format and display agent information
                                let info = Self::format_agent_info(agents);
                                CONSOLE.writeln(info.to_string())?;
                            } else {
                                // No workflow in the conversation
                                CONSOLE.writeln(
                                    TitleFormat::failed("No workflow available")
                                        .sub_title("The active conversation does not contain a workflow")
                                        .format(),
                                )?;
                            }
                        } else {
                            // Conversation not found
                            CONSOLE.writeln(
                                TitleFormat::failed("Conversation not found")
                                    .sub_title("Unable to retrieve the active conversation")
                                    .format(),
                            )?;
                        }
                    } else {
                        // No active conversation
                        CONSOLE.writeln(
                            TitleFormat::failed("No active workflow")
                                .sub_title("Start a conversation first")
                                .format(),
                        )?;
                    }
                    
                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
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
                Command::Custom(event) => {
                    if let Err(e) = self.dispatch_event(event.into()).await {
                        CONSOLE.writeln(
                            TitleFormat::failed("Failed to execute the command.")
                                .sub_title("Command Execution")
                                .error(e.to_string())
                                .format(),
                        )?;
                    }

                    input = self.console.prompt(None).await?;
                }
            }
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
                let workflow = self.api.load(self.cli.workflow.as_deref()).await?;
                self.command.register_all(&workflow);
                let conversation_id = self.api.init(workflow).await?;
                self.state.conversation_id = Some(conversation_id.clone());

                Ok(conversation_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format_agent_info_empty() {
        // Test with empty agent list
        let agents = Vec::<Agent>::new();
        let info = UI::<MockApi>::format_agent_info(&agents);
        let formatted = info.to_string();
        
        // Verify the output contains the expected messages
        assert!(formatted.contains("Agents"));
        assert!(formatted.contains("Status"));
        assert!(formatted.contains("No agents found in active workflow"));
    }
    
    #[test]
    fn test_format_agent_info() {
        // Create a test agent with all fields populated
        let mut agent = Agent::new("test-agent");
        agent.model = Some(forge_api::ModelId::new("gpt-4"));
        agent.description = Some("Test agent description".to_string());
        agent.tool_supported = Some(true);
        agent.tools = Some(vec![forge_api::ToolName::new("tool1"), forge_api::ToolName::new("tool2")]);
        agent.subscribe = Some(vec!["event1".to_string(), "event2".to_string()]);
        agent.max_turns = Some(10);
        
        let agents = vec![agent];
        let info = UI::<MockApi>::format_agent_info(&agents);
        let formatted = info.to_string();
        
        // Verify the output contains all expected information
        assert!(formatted.contains("Agents"));
        assert!(formatted.contains("test-agent"));
        assert!(formatted.contains("gpt-4"));
        assert!(formatted.contains("Test agent description"));
        assert!(formatted.contains("Tools Supported"));
        assert!(formatted.contains("Yes"));
        assert!(formatted.contains("tool1, tool2"));
        assert!(formatted.contains("event1, event2"));
        assert!(formatted.contains("Max Turns"));
        assert!(formatted.contains("10"));
    }
    
    // Mock implementation of API trait for testing
    struct MockApi;
    
    #[async_trait::async_trait]
    impl API for MockApi {
        async fn suggestions(&self) -> anyhow::Result<Vec<forge_api::File>> {
            unimplemented!()
        }

        async fn tools(&self) -> Vec<forge_api::ToolDefinition> {
            unimplemented!()
        }

        async fn models(&self) -> anyhow::Result<Vec<forge_api::Model>> {
            unimplemented!()
        }

        async fn chat(
            &self,
            _: forge_api::ChatRequest,
        ) -> anyhow::Result<forge_stream::MpscStream<anyhow::Result<forge_api::AgentMessage<forge_api::ChatResponse>, anyhow::Error>>> {
            unimplemented!()
        }

        fn environment(&self) -> forge_api::Environment {
            unimplemented!()
        }

        async fn init(&self, _: forge_api::Workflow) -> anyhow::Result<forge_api::ConversationId> {
            unimplemented!()
        }

        async fn load(&self, _: Option<&std::path::Path>) -> anyhow::Result<forge_api::Workflow> {
            unimplemented!()
        }

        async fn conversation(
            &self,
            _: &forge_api::ConversationId,
        ) -> anyhow::Result<Option<forge_api::Conversation>> {
            unimplemented!()
        }

        async fn get_variable(
            &self,
            _: &forge_api::ConversationId,
            _: &str,
        ) -> anyhow::Result<Option<serde_json::Value>> {
            unimplemented!()
        }

        async fn set_variable(
            &self,
            _: &forge_api::ConversationId,
            _: String,
            _: serde_json::Value,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
    }
}