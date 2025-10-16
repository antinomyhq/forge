use std::fmt::Display;
use std::sync::Arc;

use anyhow::{Context, Result};
use colored::Colorize;
use convert_case::{Case, Casing};
use forge_api::{
    API, AgentId, ChatRequest, ChatResponse, Conversation, ConversationId, Event,
    InterruptionReason, Model, ModelId, Provider, Workflow,
};
use forge_app::ToolResolver;
use forge_app::utils::truncate_key;
use forge_display::MarkdownFormat;
use forge_domain::{ChatResponseContent, TitleFormat};
use forge_fs::ForgeFS;
use forge_select::ForgeSelect;
use forge_spinner::SpinnerManager;
use forge_tracker::ToolCallPayload;
use merge::Merge;
use tokio_stream::StreamExt;
use tracing::debug;

use crate::cli::{Cli, ExtensionCommand, ListCommand, McpCommand, SessionCommand, TopLevelCommand};
use crate::config::ConfigManager;
use crate::conversation_selector::ConversationSelector;
use crate::env::{get_agent_from_env, get_conversation_id_from_env};
use crate::info::Info;
use crate::input::Console;
use crate::model::{CliModel, CliProvider, Command, ForgeCommandManager, PartialEvent};
use crate::prompt::ForgePrompt;
use crate::state::UIState;
use crate::title_display::TitleDisplayExt;
use crate::tools_display::format_tools;
use crate::update::on_update;
use crate::{TRACKER, banner, tracker};

pub struct UI<A, F: Fn() -> A> {
    markdown: MarkdownFormat,
    state: UIState,
    api: Arc<F::Output>,
    new_api: Arc<F>,
    console: Console,
    command: Arc<ForgeCommandManager>,
    cli: Cli,
    spinner: SpinnerManager,
    #[allow(dead_code)] // The guard is kept alive by being held in the struct
    _guard: forge_tracker::Guard,
}

impl<A: API + 'static, F: Fn() -> A> UI<A, F> {
    /// Writes a line to the console output
    /// Takes anything that implements ToString trait
    fn writeln<T: ToString>(&mut self, content: T) -> anyhow::Result<()> {
        self.spinner.write_ln(content)
    }

    /// Writes a TitleFormat to the console output with proper formatting
    fn writeln_title(&mut self, title: TitleFormat) -> anyhow::Result<()> {
        self.spinner.write_ln(title.display())
    }

    /// Retrieve available models
    async fn get_models(&mut self) -> Result<Vec<Model>> {
        self.spinner.start(Some("Loading"))?;
        let models = self.api.models().await?;
        self.spinner.stop(None)?;
        Ok(models)
    }

    /// Displays banner only if user is in interactive mode.
    fn display_banner(&self) -> Result<()> {
        if self.cli.is_interactive() {
            banner::display(false)?;
        }
        Ok(())
    }

    // Handle creating a new conversation
    async fn on_new(&mut self) -> Result<()> {
        self.api = Arc::new((self.new_api)());
        self.init_state(false).await?;

        // Reset previously set CLI parameters by the user
        self.cli.conversation = None;

        self.display_banner()?;
        self.trace_user();
        self.hydrate_caches();
        Ok(())
    }

    // Set the current mode and update conversation variable
    async fn on_agent_change(&mut self, agent_id: AgentId) -> Result<()> {
        // Convert string to AgentId for validation
        let agent = self
            .api
            .get_agents()
            .await?
            .iter()
            .find(|agent| agent.id == agent_id)
            .cloned()
            .ok_or(anyhow::anyhow!("Undefined agent: {agent_id}"))?;

        // Update the app config with the new operating agent.
        self.api.set_operating_agent(agent.id.clone()).await?;
        let name = agent.id.as_str().to_case(Case::UpperSnake).bold();

        let title = format!(
            "∙ {}",
            agent.title.as_deref().unwrap_or("<Missing agent.title>")
        )
        .dimmed();
        self.writeln_title(TitleFormat::action(format!("{name} {title}")))?;

        Ok(())
    }

    pub fn init(cli: Cli, f: F) -> Result<Self> {
        // Parse CLI arguments first to get flags
        let api = Arc::new(f());
        let env = api.environment();
        let command = Arc::new(ForgeCommandManager::default());
        Ok(Self {
            state: Default::default(),
            api,
            new_api: Arc::new(f),
            console: Console::new(env.clone(), command.clone()),
            cli,
            command,
            spinner: SpinnerManager::new(),
            markdown: MarkdownFormat::new(),
            _guard: forge_tracker::init_tracing(env.log_path(), TRACKER.clone())?,
        })
    }

    async fn prompt(&self) -> Result<Command> {
        // Get usage from current conversation if available
        let usage = if let Some(conversation_id) = &self.state.conversation_id {
            self.api
                .conversation(conversation_id)
                .await
                .ok()
                .flatten()
                .and_then(|conv| conv.context)
                .and_then(|ctx| ctx.usage)
        } else {
            None
        };

        // Prompt the user for input
        let agent_id = self.api.get_operating_agent().await.unwrap_or_default();
        let model = self.api.get_operating_model().await;
        let forge_prompt = ForgePrompt { cwd: self.state.cwd.clone(), usage, model, agent_id };
        self.console.prompt(forge_prompt).await
    }

    pub async fn run(&mut self) {
        match self.run_inner().await {
            Ok(_) => {}
            Err(error) => {
                tracing::error!(error = ?error);
                let _ = self.writeln_title(TitleFormat::error(format!("{error:?}")));
            }
        }
    }

    async fn run_inner(&mut self) -> Result<()> {
        if let Some(mcp) = self.cli.subcommands.clone() {
            return self.handle_subcommands(mcp).await;
        }

        // Display the banner in dimmed colors since we're in interactive mode
        self.display_banner()?;
        self.init_state(true).await?;
        self.trace_user();
        self.hydrate_caches();
        self.init_conversation().await?;

        // Check for dispatch flag first
        if let Some(dispatch_json) = self.cli.event.clone() {
            return self.handle_dispatch(dispatch_json).await;
        }

        // Handle direct prompt if provided
        let prompt = self.cli.prompt.clone();
        if let Some(prompt) = prompt {
            self.spinner.start(None)?;
            self.on_message(Some(prompt)).await?;
            return Ok(());
        }

        // Get initial input from file or prompt
        let mut command = match &self.cli.command {
            Some(path) => self.console.upload(path).await,
            None => self.prompt().await,
        };

        loop {
            match command {
                Ok(command) => {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            tracing::info!("User interrupted operation with Ctrl+C");
                        }
                        result = self.on_command(command) => {
                            match result {
                                Ok(exit) => if exit {return Ok(())},
                                Err(error) => {
                                    if let Some(conversation_id) = self.state.conversation_id.as_ref()
                                        && let Some(conversation) = self.api.conversation(conversation_id).await.ok().flatten() {
                                            TRACKER.set_conversation(conversation).await;
                                        }
                                    tracker::error(&error);
                                    tracing::error!(error = ?error);
                                    self.spinner.stop(None)?;
                                    self.writeln_title(TitleFormat::error(format!("{error:?}")))?;
                                },
                            }
                        }
                    }

                    self.spinner.stop(None)?;
                }
                Err(error) => {
                    tracker::error(&error);
                    tracing::error!(error = ?error);
                    self.spinner.stop(None)?;
                    self.writeln_title(TitleFormat::error(format!("{error:?}")))?;
                }
            }
            // Centralized prompt call at the end of the loop
            command = self.prompt().await;
        }
    }

    // Improve startup time by hydrating caches
    fn hydrate_caches(&self) {
        let api = self.api.clone();
        tokio::spawn(async move { api.models().await });
        let api = self.api.clone();
        tokio::spawn(async move { api.tools().await });
        let api = self.api.clone();
        tokio::spawn(async move { api.get_agents().await });
    }

    async fn handle_generate_conversation_id(&mut self) -> Result<()> {
        let conversation_id = forge_domain::ConversationId::generate();
        println!("{}", conversation_id.into_string());
        Ok(())
    }

    async fn handle_subcommands(&mut self, subcommand: TopLevelCommand) -> anyhow::Result<()> {
        match subcommand {
            TopLevelCommand::List(list_group) => {
                let porcelain = list_group.porcelain;
                match list_group.command {
                    ListCommand::Agents => {
                        self.on_show_agents(porcelain).await?;
                    }
                    ListCommand::Providers => {
                        self.on_show_providers(porcelain).await?;
                    }
                    ListCommand::Models => {
                        self.on_show_models(porcelain).await?;
                    }
                    ListCommand::Commands => {
                        self.on_show_commands(porcelain).await?;
                    }
                    ListCommand::Config => {
                        self.on_show_config(porcelain).await?;
                    }
                    ListCommand::Tools { agent } => {
                        self.on_show_tools(agent, porcelain).await?;
                    }
                    ListCommand::Mcp => {
                        self.on_show_mcp_servers(porcelain).await?;
                    }
                    ListCommand::Session => {
                        self.on_show_conversations(porcelain).await?;
                    }
                }
                return Ok(());
            }
            TopLevelCommand::Extension(extension_group) => {
                match extension_group.command {
                    ExtensionCommand::Zsh => {
                        self.on_zsh_prompt().await?;
                    }
                }
                return Ok(());
            }

            TopLevelCommand::Mcp(mcp_command) => match mcp_command.command {
                McpCommand::Import(import_args) => {
                    let scope: forge_domain::Scope = import_args.scope.into();

                    // Parse the incoming MCP configuration
                    let incoming_config: forge_domain::McpConfig = serde_json::from_str(&import_args.json)
                        .context("Failed to parse MCP configuration JSON. Expected format: {\"mcpServers\": {...}}")?;

                    // Read only the scope-specific config (not merged)
                    let mut scope_config = self.api.read_mcp_config(Some(&scope)).await?;

                    // Merge the incoming servers with scope-specific config only
                    let mut added_servers = Vec::new();
                    for (server_name, server_config) in incoming_config.mcp_servers {
                        scope_config
                            .mcp_servers
                            .insert(server_name.clone(), server_config);
                        added_servers.push(server_name);
                    }

                    // Write back to the specific scope only
                    self.api.write_mcp_config(&scope, &scope_config).await?;

                    // Log each added server after successful write
                    for server_name in added_servers {
                        self.writeln_title(TitleFormat::info(format!(
                            "Added MCP server '{server_name}'"
                        )))?;
                    }
                }
                McpCommand::List => {
                    self.on_show_mcp_servers(mcp_command.porcelain).await?;
                }
                McpCommand::Remove(rm) => {
                    let name = forge_api::ServerName::from(rm.name);
                    let scope: forge_domain::Scope = rm.scope.into();

                    // Read only the scope-specific config (not merged)
                    let mut scope_config = self.api.read_mcp_config(Some(&scope)).await?;

                    // Remove the server from scope-specific config only
                    scope_config.mcp_servers.remove(&name);

                    // Write back to the specific scope only
                    self.api.write_mcp_config(&scope, &scope_config).await?;

                    self.writeln_title(TitleFormat::info(format!("Removed server: {name}")))?;
                }
                McpCommand::Show(val) => {
                    let name = forge_api::ServerName::from(val.name);
                    let config = self.api.read_mcp_config(None).await?;
                    let server = config
                        .mcp_servers
                        .get(&name)
                        .ok_or(anyhow::anyhow!("Server not found"))?;

                    let mut output = String::new();
                    output.push_str(&format!("{name}: {server}"));
                    self.writeln_title(TitleFormat::info(output))?;
                }
                McpCommand::Reload => {
                    self.spinner.start(Some("Reloading MCPs"))?;
                    self.api.reload_mcp().await?;
                    self.writeln_title(TitleFormat::info("MCP reloaded"))?;
                }
            },
            TopLevelCommand::Info { porcelain } => {
                // Make sure to init model
                self.on_new().await?;

                self.on_info(porcelain).await?;
                return Ok(());
            }
            TopLevelCommand::Banner => {
                banner::display(true)?;
                return Ok(());
            }
            TopLevelCommand::Config(config_group) => {
                let config_manager = ConfigManager::new(self.api.clone());
                config_manager
                    .handle_command(config_group.command.clone(), config_group.porcelain)
                    .await?;
                return Ok(());
            }

            TopLevelCommand::Session(session_group) => {
                self.handle_session_command(session_group).await?;
                return Ok(());
            }
        }
        Ok(())
    }

    async fn handle_session_command(
        &mut self,
        session_group: crate::cli::SessionCommandGroup,
    ) -> anyhow::Result<()> {
        use forge_domain::ConversationId;

        match session_group.command {
            SessionCommand::List => {
                self.on_show_conversations(session_group.porcelain).await?;
            }
            SessionCommand::New => {
                self.handle_generate_conversation_id().await?;
            }
            SessionCommand::Dump { id, format } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                self.validate_session_exists(&conversation_id).await?;

                let original_id = self.state.conversation_id;
                self.state.conversation_id = Some(conversation_id);

                self.spinner.start(Some("Dumping"))?;
                self.on_dump(format).await?;

                self.state.conversation_id = original_id;
            }
            SessionCommand::Compact { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                self.validate_session_exists(&conversation_id).await?;

                let original_id = self.state.conversation_id;
                self.state.conversation_id = Some(conversation_id);

                self.spinner.start(Some("Compacting"))?;
                self.on_compaction().await?;

                self.state.conversation_id = original_id;
            }
            SessionCommand::Retry { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                self.validate_session_exists(&conversation_id).await?;

                let original_id = self.state.conversation_id;
                self.state.conversation_id = Some(conversation_id);

                self.spinner.start(None)?;
                self.on_message(None).await?;

                self.state.conversation_id = original_id;
            }
            SessionCommand::Resume { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                self.validate_session_exists(&conversation_id).await?;

                self.state.conversation_id = Some(conversation_id);
                self.writeln_title(TitleFormat::info(format!("Resumed conversation: {}", id)))?;
                // Interactive mode will be handled by the main loop
            }
        }

        Ok(())
    }

    async fn validate_session_exists(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<()> {
        let conversation = self.api.conversation(conversation_id).await?;

        if conversation.is_none() {
            anyhow::bail!(
                "Conversation '{}' not found. Use 'forge session list' to see available conversations.",
                conversation_id
            );
        }

        Ok(())
    }

    async fn on_show_agents(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let agents = self.api.get_agents().await?;

        if agents.is_empty() {
            return Ok(());
        }

        let mut info = Info::new().add_title("AGENTS");

        for agent in agents.iter() {
            let id = agent.id.as_str().to_string();
            let title = agent
                .title
                .as_deref()
                .unwrap_or("<Missing agent.title>")
                .lines()
                .collect::<Vec<_>>()
                .join(" ");
            info = info.add_title(id).add_key_value("Description", title);
        }

        self.write_info_or_porcelain(info, porcelain, true)?;

        Ok(())
    }

    /// Lists all the providers
    async fn on_show_providers(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let providers = self.api.providers().await?;

        if providers.is_empty() {
            return Ok(());
        }

        let mut info = Info::new().add_title("PROVIDERS");

        for provider in providers.iter() {
            let id = provider.id.to_string();
            let domain = provider
                .url
                .domain()
                .map(|d| format!("[{}]", d))
                .unwrap_or_default();
            info = info.add_title(id).add_key_value("Domain", domain);
        }

        self.write_info_or_porcelain(info, porcelain, true)?;

        Ok(())
    }

    /// Lists all the models
    async fn on_show_models(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let models = self.get_models().await?;

        if models.is_empty() {
            return Ok(());
        }

        let mut info = Info::new().add_title("MODELS");

        for model in models.iter() {
            let id = model.id.to_string();
            info = info.add_title(id);

            // Add context length if available
            if let Some(limit) = model.context_length {
                let context = if limit >= 1_000_000 {
                    format!("{}M", limit / 1_000_000)
                } else if limit >= 1000 {
                    format!("{}k", limit / 1000)
                } else {
                    format!("{limit}")
                };
                info = info.add_key_value("Context", context);
            }

            // Add tools support indicator if explicitly supported
            if model.tools_supported == Some(true) {
                info = info.add_key_value("Tools", "🛠️");
            }
        }

        self.write_info_or_porcelain(info, porcelain, true)?;

        Ok(())
    }

    /// Lists all the commands
    async fn on_show_commands(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let mut info = Info::new().add_title("COMMANDS");

        // Define base commands with their descriptions
        info = info
            .add_title("info".to_string())
            .add_key_value("Description", "Print session information")
            .add_title("provider".to_string())
            .add_key_value("Description", "Switch the providers")
            .add_title("model".to_string())
            .add_key_value("Description", "Switch the models")
            .add_title("new".to_string())
            .add_key_value("Description", "Start new conversation")
            .add_title("dump".to_string())
            .add_key_value(
                "Description",
                "Save conversation as JSON or HTML (use /dump html for HTML format)",
            )
            .add_title("conversation".to_string())
            .add_key_value(
                "Description",
                "List all conversations for the active workspace",
            )
            .add_title("retry".to_string())
            .add_key_value("Description", "Retry the last command")
            .add_title("compact".to_string())
            .add_key_value("Description", "Compact the conversation context")
            .add_title("tools".to_string())
            .add_key_value(
                "Description",
                "List all available tools with their descriptions and schema",
            );

        // Add alias commands
        info = info
            .add_title("ask".to_string())
            .add_key_value("Description", "Alias for agent SAGE")
            .add_title("plan".to_string())
            .add_key_value("Description", "Alias for agent MUSE");

        // Fetch agents and add them to the commands list
        let agents = self.api.get_agents().await?;
        for agent in agents {
            let title = agent
                .title
                .as_deref()
                .unwrap_or("<Missing agent.title>")
                .lines()
                .collect::<Vec<_>>()
                .join(" ");
            info = info
                .add_title(agent.id.to_string())
                .add_key_value("Description", title);
        }

        self.write_info_or_porcelain(info, porcelain, true)?;

        Ok(())
    }

    /// Lists current configuration values
    async fn on_show_config(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let agent = self
            .api
            .get_operating_agent()
            .await
            .map(|a| a.as_str().to_string());
        let model = self
            .api
            .get_operating_model()
            .await
            .map(|m| m.as_str().to_string());
        let provider = self.api.get_provider().await.ok().map(|p| p.id.to_string());

        let info = crate::config::build_config_info(agent, model, provider);
        self.write_info_or_porcelain(info, porcelain, false)?;
        Ok(())
    }

    /// Displays available tools for the current agent
    async fn on_show_tools(&mut self, agent_id: AgentId, porcelain: bool) -> anyhow::Result<()> {
        self.spinner.start(Some("Loading"))?;
        let all_tools = self.api.tools().await?;
        let agents = self.api.get_agents().await?;
        let agent = agents.into_iter().find(|agent| agent.id == agent_id);
        let agent_tools = if let Some(agent) = agent {
            let resolver = ToolResolver::new(all_tools.clone().into());
            resolver
                .resolve(&agent)
                .into_iter()
                .map(|def| def.name.clone())
                .collect()
        } else {
            Vec::new()
        };

        let info = format_tools(&agent_tools, &all_tools);
        self.write_info_or_porcelain(info, porcelain, false)?;

        Ok(())
    }

    /// Displays all MCP servers
    async fn on_show_mcp_servers(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let mcp_servers = self.api.read_mcp_config(None).await?;
        if mcp_servers.is_empty() {
            self.writeln_title(TitleFormat::error("No MCP servers found"))?;
            return Ok(());
        }

        let mut info = Info::new().add_title("MCP SERVERS");

        for (name, server) in mcp_servers.mcp_servers {
            info = info
                .add_title(name.clone())
                .add_key_value("Command", server.to_string());
        }

        self.write_info_or_porcelain(info, porcelain, true)?;
        Ok(())
    }

    async fn on_info(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let mut info = Info::from(&self.api.environment());

        // Fetch conversation if ID is available
        let conversation_id = get_conversation_id_from_env().or(self.state.conversation_id);
        let conversation = match conversation_id {
            Some(id) => self.api.conversation(&id).await.ok().flatten(),
            None => None,
        };

        let key_info = self.api.get_login_info().await;
        let operating_agent = self.api.get_operating_agent().await;
        let operating_model = self.api.get_operating_model().await;
        let provider_result = self.api.get_provider().await;

        // Add conversation information if available
        if let Some(conversation) = conversation {
            info = info.extend(Info::from(&conversation));
        } else {
            info = info.extend(
                Info::new()
                    .add_title("CONVERSATION")
                    .add_key_value("ID", "<Uninitialized>".to_string()),
            );
        }

        info = info.add_title("AGENT");
        if let Some(agent) = operating_agent {
            info = info.add_key_value("ID", agent.as_str().to_uppercase());
        }

        // Add model information if available
        if let Some(model) = operating_model {
            info = info.add_key_value("Model", model);
        }

        // Add provider information if available
        if let Ok(provider) = provider_result {
            info = info.add_key_value("Provider (URL)", provider.url);
            if let Some(ref api_key) = provider.key {
                info = info.add_key_value("API Key", truncate_key(api_key));
            }
        }

        // Add user information if available
        if let Some(login_info) = key_info? {
            info = info.extend(Info::from(&login_info));
        }

        self.write_info_or_porcelain(info, porcelain, false)?;

        Ok(())
    }

    /// Helper to output Info struct either as formatted display or porcelain
    ///
    /// # Arguments
    /// * `info` - The Info struct to display
    /// * `porcelain` - Whether to use porcelain mode
    /// * `title_position` - Position of the title column in porcelain mode (0 =
    ///   first, usize::MAX = last)
    /// * `include_title` - Whether to include the title in porcelain output
    ///   (false for section headers, true for IDs)
    fn write_info_or_porcelain(
        &mut self,
        info: Info,
        porcelain: bool,
        include_title: bool,
    ) -> anyhow::Result<()> {
        if porcelain {
            // Use to_rows to get key-value pairs and format with columns
            crate::cli_format::format_columns(info.to_rows(include_title));
        } else {
            self.writeln(info)?;
        }
        Ok(())
    }

    async fn on_zsh_prompt(&self) -> anyhow::Result<()> {
        println!("{}", include_str!("../../../shell-plugin/forge.plugin.zsh"));
        Ok(())
    }

    async fn list_conversations(&mut self) -> anyhow::Result<()> {
        self.spinner.start(Some("Loading Conversations"))?;
        let max_conversations = self.api.environment().max_conversations;
        let conversations = self.api.list_conversations(Some(max_conversations)).await?;
        self.spinner.stop(None)?;

        if conversations.is_empty() {
            self.writeln_title(TitleFormat::error(
                "No conversations found in this workspace.",
            ))?;
            return Ok(());
        }

        if let Some(conversation) =
            ConversationSelector::select_conversation(&conversations).await?
        {
            self.state.conversation_id = Some(conversation.id);
        }
        Ok(())
    }

    async fn on_show_conversations(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let max_conversations = self.api.environment().max_conversations;
        let conversations = self.api.list_conversations(Some(max_conversations)).await?;

        if conversations.is_empty() {
            return Ok(());
        }

        let mut info = Info::new().add_title("SESSIONS");

        for conv in conversations.into_iter() {
            if conv.title.is_none() || conv.context.is_none() {
                continue;
            }

            let title = conv.title.as_deref().unwrap();

            // Format time using humantime library (same as conversation_selector.rs)
            let duration = chrono::Utc::now().signed_duration_since(
                conv.metadata.updated_at.unwrap_or(conv.metadata.created_at),
            );
            let duration =
                std::time::Duration::from_secs((duration.num_minutes() * 60).max(0) as u64);
            let time_ago = if duration.is_zero() {
                "now".to_string()
            } else {
                format!("{} ago", humantime::format_duration(duration))
            };

            // Add conversation: Title=<title>, Updated=<time_ago>, with ID as section title
            info = info
                .add_title(title.to_string())
                .add_key_value("Id", conv.id)
                .add_key_value("Updated", time_ago);
        }

        self.write_info_or_porcelain(info, porcelain, true)?;

        Ok(())
    }

    async fn on_command(&mut self, command: Command) -> anyhow::Result<bool> {
        match command {
            Command::Conversations => {
                self.list_conversations().await?;
            }
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
                self.on_info(false).await?;
            }
            Command::Usage => {
                self.on_usage().await?;
            }
            Command::Message(ref content) => {
                self.spinner.start(None)?;
                self.on_message(Some(content.clone())).await?;
            }
            Command::Forge => {
                self.on_agent_change(AgentId::FORGE).await?;
            }
            Command::Muse => {
                self.on_agent_change(AgentId::MUSE).await?;
            }
            Command::Sage => {
                self.on_agent_change(AgentId::SAGE).await?;
            }
            Command::Help => {
                let info = Info::from(self.command.as_ref());
                self.writeln(info)?;
            }
            Command::Tools => {
                let agent_id = self.api.get_operating_agent().await.unwrap_or_default();
                self.on_show_tools(agent_id, false).await?;
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
            Command::Provider => {
                self.on_provider_selection().await?;
            }
            Command::Shell(ref command) => {
                self.api.execute_shell_command_raw(command).await?;
            }
            Command::Agent => {
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

                let agents = self.api.get_agents().await?;
                let n = agents
                    .iter()
                    .map(|a| a.id.as_str().len())
                    .max()
                    .unwrap_or_default();
                let display_agents = agents
                    .into_iter()
                    .map(|agent| {
                        let title = &agent.title.unwrap_or("<Missing agent.title>".to_string());
                        {
                            let label = format!(
                                "{:<n$} {}",
                                agent.id.as_str().bold(),
                                title.lines().collect::<Vec<_>>().join(" ").dimmed()
                            );
                            Agent { label, id: agent.id.clone() }
                        }
                    })
                    .collect::<Vec<_>>();

                if let Some(selected_agent) = ForgeSelect::select(
                    "select the agent from following list",
                    display_agents.clone(),
                )
                .prompt()?
                {
                    self.on_agent_change(selected_agent.id).await?;
                }
            }
            Command::Login => {
                self.spinner.start(Some("Logging in"))?;
                self.api.logout().await?;
                self.login().await?;
                self.spinner.stop(None)?;
                let key_info = self.api.get_login_info().await?;
                tracker::login(
                    key_info
                        .and_then(|v| v.auth_provider_id)
                        .unwrap_or_default(),
                );
            }
            Command::Logout => {
                self.spinner.start(Some("Logging out"))?;
                self.api.logout().await?;
                self.spinner.stop(None)?;
                self.writeln_title(TitleFormat::info("Logged out"))?;
                // Exit the UI after logout
                return Ok(true);
            }
            Command::Retry => {
                self.spinner.start(None)?;
                self.on_message(None).await?;
            }
            Command::AgentSwitch(agent_id) => {
                // Validate that the agent exists by checking against loaded agents
                let agents = self.api.get_agents().await?;
                let agent_exists = agents.iter().any(|agent| agent.id.as_str() == agent_id);

                if agent_exists {
                    self.on_agent_change(AgentId::new(agent_id)).await?;
                } else {
                    return Err(anyhow::anyhow!(
                        "Agent '{}' not found or unavailable",
                        agent_id
                    ));
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
        let content = TitleFormat::action(format!(
            "Context size reduced by {token_reduction:.1}% (tokens), {message_reduction:.1}% (messages)"
        ));
        self.writeln_title(content)?;
        Ok(())
    }

    /// Select a model from the available models
    /// Returns Some(ModelId) if a model was selected, or None if selection was
    /// canceled
    async fn select_model(&mut self) -> Result<Option<ModelId>> {
        // Fetch available models
        let mut models = self
            .get_models()
            .await?
            .into_iter()
            .map(CliModel)
            .collect::<Vec<_>>();

        // Sort the models by their names in ascending order
        models.sort_by(|a, b| a.0.name.cmp(&b.0.name));

        // Find the index of the current model
        let current_model = self.api.get_operating_model().await;
        let starting_cursor = current_model
            .as_ref()
            .and_then(|current| models.iter().position(|m| &m.0.id == current))
            .unwrap_or(0);

        // Use the centralized select module
        match ForgeSelect::select("Select a model:", models)
            .with_starting_cursor(starting_cursor)
            .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
            .prompt()?
        {
            Some(model) => Ok(Some(model.0.id)),
            None => Ok(None),
        }
    }

    async fn select_provider(&mut self) -> Result<Option<Provider>> {
        // Fetch available providers
        let mut providers = self
            .api
            .providers()
            .await?
            .into_iter()
            .map(CliProvider)
            .collect::<Vec<_>>();

        if providers.is_empty() {
            return Err(anyhow::anyhow!("No AI provider API keys configured"));
        }

        // Sort the providers by their display names in ascending order
        providers.sort_by_key(|a| a.to_string());

        // Find the index of the current provider
        let current_provider = self.api.get_provider().await.ok();
        let starting_cursor = current_provider
            .as_ref()
            .and_then(|current| providers.iter().position(|p| p.0.id == current.id))
            .unwrap_or(0);

        // Use the centralized select module
        match ForgeSelect::select("Select a provider:", providers)
            .with_starting_cursor(starting_cursor)
            .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
            .prompt()?
        {
            Some(provider) => Ok(Some(provider.0)),
            None => Ok(None),
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

        // Update the operating model via API
        self.api.set_operating_model(model.clone()).await?;

        // Update the UI state with the new model
        self.update_model(Some(model.clone()));

        self.writeln_title(TitleFormat::action(format!("Switched to model: {model}")))?;

        Ok(())
    }

    async fn on_provider_selection(&mut self) -> Result<()> {
        // Select a provider
        let provider_option = self.select_provider().await?;

        // If no provider was selected (user canceled), return early
        let provider = match provider_option {
            Some(provider) => provider,
            None => return Ok(()),
        };

        // Set the provider via API
        self.api.set_provider(provider.id).await?;

        self.writeln_title(TitleFormat::action(format!(
            "Switched to provider: {}",
            CliProvider(provider.clone())
        )))?;

        // Check if the current model is available for the new provider
        let current_model = self.api.get_operating_model().await;
        if let Some(current_model) = current_model {
            let models = self.get_models().await?;
            let model_available = models.iter().any(|m| m.id == current_model);

            if !model_available {
                // Prompt user to select a new model
                self.writeln_title(TitleFormat::info("Please select a new model"))?;
                self.on_model_selection().await?;
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

        self.on_chat(chat).await
    }

    /// Initializes and returns a conversation ID for the current session.
    ///
    /// Handles conversation setup for both interactive and headless modes:
    /// - **Interactive**: Reuses existing conversation, loads from file, or
    ///   creates new
    /// - **Headless**: Uses environment variables or generates new conversation
    ///
    /// Displays initialization status and updates UI state with the
    /// conversation ID.
    async fn init_conversation(&mut self) -> Result<ConversationId> {
        let mut is_new = false;
        let id = if self.cli.is_interactive() {
            self.init_conversation_interactive(&mut is_new).await?
        } else {
            self.init_conversation_headless(&mut is_new).await?
        };

        // Print if the state is being reinitialized
        if self.state.conversation_id.is_none() {
            self.print_conversation_status(is_new, id).await?;
        }

        // Always set the conversation id in state
        self.state.conversation_id = Some(id);

        Ok(id)
    }

    async fn init_conversation_interactive(
        &mut self,
        is_new: &mut bool,
    ) -> Result<ConversationId, anyhow::Error> {
        Ok(if let Some(id) = self.state.conversation_id {
            id
        } else if let Some(ref path) = self.cli.conversation {
            let conversation: Conversation =
                serde_json::from_str(ForgeFS::read_utf8(path.as_os_str()).await?.as_str())
                    .context("Failed to parse Conversation")?;
            let id = conversation.id;
            self.api.upsert_conversation(conversation).await?;
            id
        } else {
            let conversation = Conversation::generate();
            let id = conversation.id;
            *is_new = true;
            self.api.upsert_conversation(conversation).await?;
            id
        })
    }

    async fn init_conversation_headless(
        &mut self,
        is_new: &mut bool,
    ) -> Result<ConversationId, anyhow::Error> {
        Ok(if let Some(id) = self.state.conversation_id {
            id
        } else {
            if let Some(agent_id) = get_agent_from_env() {
                self.api.set_operating_agent(agent_id).await?;
            }
            if let Some(id) = get_conversation_id_from_env() {
                match self.api.conversation(&id).await? {
                    Some(conversation) => conversation.id,
                    None => {
                        let conversation = Conversation::new(id);
                        let id = conversation.id;
                        self.api.upsert_conversation(conversation).await?;
                        *is_new = true;
                        id
                    }
                }
            } else {
                let conversation = Conversation::generate();
                let id = conversation.id;
                self.api.upsert_conversation(conversation).await?;
                *is_new = true;
                id
            }
        })
    }

    async fn print_conversation_status(
        &mut self,
        new_conversation: bool,
        id: ConversationId,
    ) -> Result<(), anyhow::Error> {
        let mut title = if new_conversation {
            "Initialize".to_string()
        } else {
            "Continue".to_string()
        };

        title.push_str(format!(" {}", id.into_string()).as_str());

        let mut sub_title = String::new();
        sub_title.push('[');

        if let Some(ref agent) = self.api.get_operating_agent().await {
            sub_title.push_str(format!("via {}", agent).as_str());
        }

        if let Some(ref model) = self.api.get_operating_model().await {
            sub_title.push_str(format!("/{}", model.as_str()).as_str());
        }

        sub_title.push(']');

        self.writeln_title(TitleFormat::debug(title).sub_title(sub_title.bold().to_string()))?;
        Ok(())
    }

    /// Initialize the state of the UI
    async fn init_state(&mut self, first: bool) -> Result<Workflow> {
        // Run the independent initialization tasks in parallel for better performance
        let workflow = self.api.read_workflow(self.cli.workflow.as_deref()).await?;

        // Ensure we have a model selected before proceeding with initialization
        if self.api.get_operating_model().await.is_none() {
            let model = self
                .select_model()
                .await?
                .ok_or(anyhow::anyhow!("Model selection is required to continue"))?;
            self.api.set_operating_model(model).await?;
        }

        // Create base workflow and trigger updates if this is the first initialization
        let mut base_workflow = Workflow::default();
        base_workflow.merge(workflow.clone());
        if first {
            // only call on_update if this is the first initialization
            on_update(self.api.clone(), base_workflow.updates.as_ref()).await;
        }

        // Execute independent operations in parallel to improve performance
        let write_workflow_fut = self
            .api
            .write_workflow(self.cli.workflow.as_deref(), &workflow);
        let get_agents_fut = self.api.get_agents();
        let get_operating_agent_fut = self.api.get_operating_agent();

        let (write_workflow_result, agents_result, _operating_agent_result) =
            tokio::join!(write_workflow_fut, get_agents_fut, get_operating_agent_fut);

        // Handle workflow write result first as it's critical for the system state
        write_workflow_result?;

        // Register agent commands with proper error handling and user feedback
        match agents_result {
            Ok(agents) => {
                let registration_result = self.command.register_agent_commands(agents);

                // Show warning for any skipped agents due to conflicts
                for skipped_command in registration_result.skipped_conflicts {
                    self.writeln_title(TitleFormat::error(format!(
                        "Skipped agent command '{skipped_command}' due to name conflict with built-in command"
                    )))?;
                }
            }
            Err(e) => {
                self.writeln_title(TitleFormat::error(format!(
                    "Failed to load agents for command registration: {e}"
                )))?;
            }
        }

        // Finalize UI state initialization by registering commands and setting up the
        // state
        self.command.register_all(&base_workflow);
        let operating_model = self.api.get_operating_model().await;
        self.state = UIState::new(self.api.environment());
        self.update_model(operating_model);

        Ok(workflow)
    }

    async fn login(&mut self) -> Result<()> {
        let auth = self.api.init_login().await?;
        open::that(auth.auth_url.as_str()).ok();
        self.writeln_title(TitleFormat::info(
            format!("Login here: {}", auth.auth_url).as_str(),
        ))?;
        self.spinner.start(Some("Waiting for login to complete"))?;

        self.api.login(&auth).await?;

        self.spinner.stop(None)?;

        self.writeln_title(TitleFormat::info("Login completed".to_string().as_str()))?;

        Ok(())
    }

    async fn on_message(&mut self, content: Option<String>) -> Result<()> {
        let conversation_id = self.init_conversation().await?;

        // Create a ChatRequest with the appropriate event type
        let operating_agent = self.api.get_operating_agent().await.unwrap_or_default();
        let event = Event::new(format!("{operating_agent}"), content);

        // Create the chat request with the event
        let chat = ChatRequest::new(event, conversation_id);

        self.on_chat(chat).await
    }

    async fn on_chat(&mut self, chat: ChatRequest) -> Result<()> {
        let mut stream = self.api.chat(chat).await?;

        while let Some(message) = stream.next().await {
            match message {
                Ok(message) => self.handle_chat_response(message).await?,
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
        if let Some(conversation_id) = self.state.conversation_id {
            let conversation = self.api.conversation(&conversation_id).await?;
            if let Some(conversation) = conversation {
                let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
                if let Some(format) = format {
                    if format == "html" {
                        // Export as HTML
                        let html_content = conversation.to_html();
                        let path = format!("{timestamp}-dump.html");
                        tokio::fs::write(path.as_str(), html_content).await?;

                        self.writeln_title(
                            TitleFormat::action("Conversation HTML dump created".to_string())
                                .sub_title(path.to_string()),
                        )?;

                        if self.api.environment().auto_open_dump {
                            open::that(path.as_str()).ok();
                        }

                        return Ok(());
                    }
                } else {
                    // Default: Export as JSON
                    let path = format!("{timestamp}-dump.json");
                    let content = serde_json::to_string_pretty(&conversation)?;
                    tokio::fs::write(path.as_str(), content).await?;

                    self.writeln_title(
                        TitleFormat::action("Conversation JSON dump created".to_string())
                            .sub_title(path.to_string()),
                    )?;

                    if self.api.environment().auto_open_dump {
                        open::that(path.as_str()).ok();
                    }
                };
            } else {
                return Err(anyhow::anyhow!("Could not create dump"))
                    .context(format!("Conversation: {conversation_id} was not found"));
            }
        } else {
            return Err(anyhow::anyhow!("No conversation initiated yet"))
                .context("Could not create dump");
        }
        Ok(())
    }

    async fn handle_chat_response(&mut self, message: ChatResponse) -> Result<()> {
        debug!(chat_response = ?message, "Chat Response");
        if message.is_empty() {
            return Ok(());
        }

        match message {
            ChatResponse::TaskMessage { content } => match content {
                ChatResponseContent::Title(title) => self.writeln(title.display())?,
                ChatResponseContent::PlainText(text) => self.writeln(text)?,
                ChatResponseContent::Markdown(text) => {
                    tracing::info!(message = %text, "Agent Response");
                    self.writeln(self.markdown.render(&text))?;
                }
            },
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
                tracker::tool_call(payload);

                self.spinner.start(None)?;
                if !self.cli.verbose {
                    return Ok(());
                }
            }
            ChatResponse::Usage(_) => {}
            ChatResponse::RetryAttempt { cause, duration: _ } => {
                if !self.api.environment().retry_config.suppress_retry_errors {
                    self.spinner.start(Some("Retrying"))?;
                    self.writeln_title(TitleFormat::error(cause.as_str()))?;
                }
            }
            ChatResponse::Interrupt { reason } => {
                self.spinner.stop(None)?;

                let title = match reason {
                    InterruptionReason::MaxRequestPerTurnLimitReached { limit } => {
                        format!("Maximum request ({limit}) per turn achieved")
                    }
                    InterruptionReason::MaxToolFailurePerTurnLimitReached { limit, .. } => {
                        format!("Maximum tool failure limit ({limit}) reached for this turn")
                    }
                };

                self.writeln_title(TitleFormat::action(title))?;
                self.should_continue().await?;
            }
            ChatResponse::TaskReasoning { content } => {
                if !content.trim().is_empty() {
                    let rendered_content = self.markdown.render(&content);
                    self.writeln(rendered_content.dimmed())?;
                }
            }
            ChatResponse::TaskComplete => {
                if let Some(conversation_id) = self.state.conversation_id {
                    self.on_completion(conversation_id).await?;
                }
            }
        }
        Ok(())
    }

    async fn should_continue(&mut self) -> anyhow::Result<()> {
        let should_continue = ForgeSelect::confirm("Do you want to continue anyway?")
            .with_default(true)
            .prompt()?;

        if should_continue.unwrap_or(false) {
            self.spinner.start(None)?;
            Box::pin(self.on_message(None)).await?;
        }

        Ok(())
    }

    async fn on_completion(&mut self, conversation_id: ConversationId) -> anyhow::Result<()> {
        self.spinner.start(Some("Loading Summary"))?;
        let conversation = self
            .api
            .conversation(&conversation_id)
            .await?
            .ok_or(anyhow::anyhow!("Conversation not found: {conversation_id}"))?;

        let info = Info::default().extend(&conversation);

        self.writeln(info)?;

        self.spinner.stop(None)?;

        // Only prompt for new conversation if in interactive mode
        if self.cli.is_interactive() {
            let prompt_text = "Start a new conversation?";
            let should_start_new_chat = ForgeSelect::confirm(prompt_text)
                // Pressing ENTER should start new
                .with_default(true)
                .with_help_message("ESC = No, continue current conversation")
                .prompt()
                // Cancel or failure should continue with the session
                .unwrap_or(Some(false))
                .unwrap_or(false);

            // if conversation is over
            if should_start_new_chat {
                self.on_new().await?;
            }
        }

        Ok(())
    }

    fn update_model(&mut self, model: Option<ModelId>) {
        if let Some(ref model) = model {
            tracker::set_model(model.to_string());
        }
    }

    async fn on_custom_event(&mut self, event: Event) -> Result<()> {
        let conversation_id = self.init_conversation().await?;
        let chat = ChatRequest::new(event, conversation_id);
        self.on_chat(chat).await
    }

    async fn on_usage(&mut self) -> anyhow::Result<()> {
        self.spinner.start(Some("Loading Usage"))?;

        // Get usage from current conversation if available
        let conversation_usage = if let Some(conversation_id) = &self.state.conversation_id {
            self.api
                .conversation(conversation_id)
                .await
                .ok()
                .flatten()
                .and_then(|conv| conv.context)
                .and_then(|ctx| ctx.usage)
        } else {
            None
        };

        let mut info = if let Some(usage) = conversation_usage {
            Info::from(&usage)
        } else {
            Info::new()
        };

        if let Ok(Some(user_usage)) = self.api.user_usage().await {
            info = info.extend(Info::from(&user_usage));
        }

        self.writeln(info)?;
        self.spinner.stop(None)?;
        Ok(())
    }

    fn trace_user(&self) {
        let api = self.api.clone();
        // NOTE: Spawning required so that we don't block the user while querying user
        // info
        tokio::spawn(async move {
            if let Ok(Some(user_info)) = api.user_info().await {
                tracker::login(user_info.auth_provider_id.into_string());
            }
        });
    }
}
