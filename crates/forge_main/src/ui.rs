use std::sync::Arc;

use anyhow::Result;
use chrono::{Local, LocalResult, TimeZone, Utc};
use colored::Colorize;
use forge_api::{AgentMessage, ChatRequest, ChatResponse, ConversationId, Event, Model, API};
use forge_display::TitleFormat;
use forge_snaps::Snapshot;
use lazy_static::lazy_static;
use serde::Deserialize;
use serde_json::Value;
use tokio_stream::StreamExt;
use tracing::error;

use crate::banner;
use crate::cli::{Cli, SnapshotCommand};
use crate::console::CONSOLE;
use crate::info::{Info, UsageInfo};
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

#[derive(Deserialize)]
struct PartialEvent {
    pub name: String,
    pub value: String,
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
        // Check for dispatch flag first
        if let Some(dispatch_json) = self.cli.event.clone() {
            return self.handle_dispatch(dispatch_json).await;
        }

        if let Some(sub_command) = self.cli.snapshot.as_ref() {
            return self.handle_snaps(sub_command).await;
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
                        Info::from(&self.api.environment()).extend(Info::from(UsageInfo::new(
                            &self.state.usage,
                            self.api.list_snapshots(None).await?.len(),
                        )));

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
    async fn handle_snaps(&self, snapshot_command: &SnapshotCommand) -> Result<()> {
        match snapshot_command {
            SnapshotCommand::List { path } => {
                let snapshots: Vec<Snapshot> = self
                    .api
                    .list_snapshots(path.as_ref().map(|v| v.as_path()))
                    .await?;
                if snapshots.is_empty() {
                    CONSOLE.writeln(
                        TitleFormat::failed("Snapshots")
                            .sub_title("No snapshots found")
                            .format(),
                    )?;
                    return Ok(());
                }

                CONSOLE.writeln(
                    TitleFormat::success(format!("Found {} snapshots", snapshots.len())).format(),
                )?;
                CONSOLE.newline()?;

                for (i, snap) in snapshots.iter().enumerate() {
                    // Create a title with the index and timestamp
                    let mut title = TitleFormat::execute(format!("Snapshot #{}", i));

                    if let Ok(time) = i64::try_from(snap.timestamp) {
                        if let LocalResult::Single(datetime_utc) = Utc.timestamp_millis_opt(time) {
                            let datetime_local = datetime_utc
                                .with_timezone(&Local)
                                .format("%H:%M:%S %Y-%m-%d")
                                .to_string();
                            title = title.sub_title(format!("timestamp: {}", datetime_local));
                        }
                    }
                    CONSOLE.writeln(title.format())?;

                    CONSOLE.writeln(format!(
                        "{}: {}",
                        "Snapshot Timestamp".bold(),
                        snap.timestamp
                    ))?;

                    CONSOLE.writeln(format!("{}: {}", "Id".bold(), snap.id))?;
                    CONSOLE.writeln(format!(
                        "{}: '{}'",
                        "Original Path".bold(),
                        snap.original_path
                    ))?;

                    // Add a separator between snapshots
                    if i < snapshots.len() - 1 {
                        CONSOLE.writeln("---".dimmed().to_string())?;
                    }
                }
                Ok(())
            }
            SnapshotCommand::Restore { timestamp, path, hash } => {
                let result_title = TitleFormat::execute("Snapshot Restore");

                if let Some(timestamp) = timestamp {
                    CONSOLE.writeln(
                        result_title
                            .sub_title(format!("restoring by timestamp: {}", timestamp))
                            .format(),
                    )?;

                    self.api.restore_by_timestamp(path, *timestamp).await?;

                    CONSOLE.writeln(
                        TitleFormat::success("Restore Complete")
                            .sub_title(format!("path: {}", path.display()))
                            .format(),
                    )?;
                    return Ok(());
                }

                if let Some(hash) = hash {
                    CONSOLE.writeln(
                        result_title
                            .sub_title(format!("restoring by hash: {}", hash))
                            .format(),
                    )?;

                    self.api.restore_by_hash(path, hash).await?;

                    CONSOLE.writeln(
                        TitleFormat::success("Restore Complete")
                            .sub_title(format!("path: {}", path.display()))
                            .format(),
                    )?;
                    return Ok(());
                }

                CONSOLE.writeln(
                    result_title
                        .sub_title("restoring previous version")
                        .format(),
                )?;

                self.api.restore_previous(path).await?;

                CONSOLE.writeln(
                    TitleFormat::success("Restore Complete")
                        .sub_title(format!("path: {}", path.display()))
                        .format(),
                )?;
                Ok(())
            }
            SnapshotCommand::Diff { path, timestamp, hash } => {
                let metadata = if let Some(timestamp) = timestamp {
                    CONSOLE.writeln(
                        TitleFormat::execute("Snapshot Diff")
                            .sub_title(format!("comparing with timestamp: {}", timestamp))
                            .format(),
                    )?;

                    self.api.get_snapshot_by_timestamp(path, *timestamp).await?
                } else if let Some(hash) = hash {
                    CONSOLE.writeln(
                        TitleFormat::execute("Snapshot Diff")
                            .sub_title(format!("comparing with hash: {}", hash))
                            .format(),
                    )?;

                    self.api.get_snapshot_by_hash(path, hash).await?
                } else {
                    CONSOLE.writeln(
                        TitleFormat::execute("Snapshot Diff")
                            .sub_title("comparing with previous version")
                            .format(),
                    )?;

                    self.api.get_latest_snapshot(path).await?
                };

                let prev_content = metadata.content;
                let cur_content = String::from_utf8(forge_fs::ForgeFS::read(path).await?)?;
                let diff = forge_display::DiffFormat::format(
                    "diff",
                    path.to_path_buf(),
                    &prev_content,
                    &cur_content,
                );

                CONSOLE.writeln(diff)?;

                Ok(())
            }
            SnapshotCommand::Purge { older_than } => {
                let mut title = TitleFormat::execute("Purging Snapshots");
                if *older_than == 0 {
                    title = title.sub_title("of all time.".to_string());
                } else {
                    title = title.sub_title(format!("older than {} days", older_than));
                }

                CONSOLE.writeln(title.format())?;

                let count = self.api.purge_older_than(*older_than).await?;

                CONSOLE.writeln(
                    TitleFormat::success("Purge Complete")
                        .sub_title(format!("deleted {} snapshots", count))
                        .format(),
                )?;

                Ok(())
            }
        }
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
        match self.api.chat(chat).await {
            Ok(mut stream) => self.handle_chat_stream(&mut stream).await,
            Err(err) => Err(err),
        }
    }
}
