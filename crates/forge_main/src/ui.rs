use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use colored::Colorize;
use convert_case::{Case, Casing};
use forge_api::{
    API, AgentId, AnyProvider, ApiKeyRequest, AuthContextRequest, AuthContextResponse, ChatRequest,
    ChatResponse, CodeRequest, Conversation, ConversationId, DeviceCodeRequest, Event,
    InterruptionReason, Model, ModelId, Provider, ProviderId, TextMessage, UserPrompt, Workflow,
};
use forge_app::ToolResolver;
use forge_app::utils::truncate_key;
use forge_display::MarkdownFormat;
use forge_domain::{
    AuthMethod, ChatResponseContent, ContextMessage, Role, TitleFormat, UserCommand,
};
use forge_fs::ForgeFS;
use forge_select::ForgeSelect;
use forge_spinner::SpinnerManager;
use forge_tracker::ToolCallPayload;
use merge::Merge;
use strum::IntoEnumIterator;
use tokio_stream::StreamExt;
use tracing::debug;
use url::Url;

use crate::cli::{
    Cli, ConversationCommand, ExtensionCommand, ListCommand, McpCommand, TopLevelCommand,
};
use crate::conversation_selector::ConversationSelector;
use crate::env::should_show_completion_prompt;
use crate::info::Info;
use crate::input::Console;
use crate::model::{CliModel, CliProvider, ForgeCommandManager, SlashCommand};
use crate::porcelain::Porcelain;
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
        let models = self.api.get_models().await?;
        self.spinner.stop(None)?;
        Ok(models)
    }

    /// Helper to get provider for an optional agent, defaulting to the current
    /// active agent's provider
    async fn get_provider(&self, agent_id: Option<AgentId>) -> Result<Provider<Url>> {
        match agent_id {
            Some(id) => self.api.get_agent_provider(id).await,
            None => self.api.get_default_provider().await,
        }
    }

    /// Helper to get model for an optional agent, defaulting to the current
    /// active agent's model
    async fn get_agent_model(&self, agent_id: Option<AgentId>) -> Option<ModelId> {
        match agent_id {
            Some(id) => self.api.get_agent_model(id).await,
            None => self.api.get_default_model().await,
        }
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

        // Set agent if provided via CLI
        if let Some(agent_id) = self.cli.agent.clone() {
            self.api.set_active_agent(agent_id).await?;
        }

        // Reset previously set CLI parameters by the user
        self.cli.conversation = None;
        self.cli.conversation_id = None;

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
        self.api.set_active_agent(agent.id.clone()).await?;
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

    async fn prompt(&self) -> Result<SlashCommand> {
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
        let agent_id = self.api.get_active_agent().await.unwrap_or_default();
        let model = self
            .get_agent_model(self.api.get_active_agent().await)
            .await;
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
        tokio::spawn(async move { api.get_models().await });
        let api = self.api.clone();
        tokio::spawn(async move { api.get_tools().await });
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
                    ListCommand::Agent => {
                        self.on_show_agents(porcelain).await?;
                    }
                    ListCommand::Provider => {
                        self.on_show_providers(porcelain).await?;
                    }
                    ListCommand::Model => {
                        self.on_show_models(porcelain).await?;
                    }
                    ListCommand::Command => {
                        self.on_show_commands(porcelain).await?;
                    }
                    ListCommand::Config => {
                        self.on_show_config(porcelain).await?;
                    }
                    ListCommand::Tool { agent } => {
                        self.on_show_tools(agent, porcelain).await?;
                    }
                    ListCommand::Mcp => {
                        self.on_show_mcp_servers(porcelain).await?;
                    }
                    ListCommand::Conversation => {
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
            TopLevelCommand::Info { porcelain, conversation_id } => {
                // Make sure to init model
                self.on_new().await?;

                let conversation_id = conversation_id
                    .as_deref()
                    .map(ConversationId::parse)
                    .transpose()?;

                self.on_info(porcelain, conversation_id).await?;
                return Ok(());
            }
            TopLevelCommand::Env => {
                self.on_env().await?;
                return Ok(());
            }
            TopLevelCommand::Banner => {
                banner::display(true)?;
                return Ok(());
            }
            TopLevelCommand::Config(config_group) => {
                self.handle_config_command(config_group.command.clone(), config_group.porcelain)
                    .await?;
                return Ok(());
            }

            TopLevelCommand::Provider(provider_group) => {
                self.handle_provider_command(provider_group).await?;
                return Ok(());
            }

            TopLevelCommand::Conversation(conversation_group) => {
                self.handle_conversation_command(conversation_group).await?;
                return Ok(());
            }

            TopLevelCommand::Suggest { prompt } => {
                self.on_cmd(UserPrompt::from(prompt)).await?;
                return Ok(());
            }
        }
        Ok(())
    }

    async fn handle_conversation_command(
        &mut self,
        conversation_group: crate::cli::ConversationCommandGroup,
    ) -> anyhow::Result<()> {
        use forge_domain::ConversationId;

        match conversation_group.command {
            ConversationCommand::List => {
                self.on_show_conversations(conversation_group.porcelain)
                    .await?;
            }
            ConversationCommand::New => {
                self.handle_generate_conversation_id().await?;
            }
            ConversationCommand::Dump { id, format } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                self.validate_conversation_exists(&conversation_id).await?;

                let original_id = self.state.conversation_id;
                self.state.conversation_id = Some(conversation_id);

                self.spinner.start(Some("Dumping"))?;
                self.on_dump(format).await?;

                self.state.conversation_id = original_id;
            }
            ConversationCommand::Compact { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                self.validate_conversation_exists(&conversation_id).await?;

                let original_id = self.state.conversation_id;
                self.state.conversation_id = Some(conversation_id);

                self.spinner.start(Some("Compacting"))?;
                self.on_compaction().await?;

                self.state.conversation_id = original_id;
            }
            ConversationCommand::Retry { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                self.validate_conversation_exists(&conversation_id).await?;

                let original_id = self.state.conversation_id;
                self.state.conversation_id = Some(conversation_id);

                self.spinner.start(None)?;
                self.on_message(None).await?;

                self.state.conversation_id = original_id;
            }
            ConversationCommand::Resume { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                self.validate_conversation_exists(&conversation_id).await?;

                self.state.conversation_id = Some(conversation_id);
                self.writeln_title(TitleFormat::info(format!("Resumed conversation: {}", id)))?;
                // Interactive mode will be handled by the main loop
            }
            ConversationCommand::Show { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                let conversation = self.validate_conversation_exists(&conversation_id).await?;

                self.on_show_last_message(conversation).await?;
            }
            ConversationCommand::Info { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                let conversation = self.validate_conversation_exists(&conversation_id).await?;

                self.on_show_conv_info(conversation).await?;
            }
            ConversationCommand::Clone { id } => {
                let conversation_id = ConversationId::parse(&id)
                    .context(format!("Invalid conversation ID: {}", id))?;

                let conversation = self.validate_conversation_exists(&conversation_id).await?;

                self.spinner.start(Some("Cloning"))?;
                self.on_clone_conversation(conversation, conversation_group.porcelain)
                    .await?;
                self.spinner.stop(None)?;
            }
        }

        Ok(())
    }

    async fn validate_conversation_exists(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Conversation> {
        let conversation = self.api.conversation(conversation_id).await?;

        conversation.ok_or_else(|| {
            anyhow::anyhow!(
                "Conversation '{}' not found. Use 'forge conversation list' to see available conversations.",
                conversation_id
            )
        })
    }

    async fn handle_provider_command(
        &mut self,
        provider_group: crate::cli::ProviderCommandGroup,
    ) -> anyhow::Result<()> {
        use crate::cli::ProviderCommand;

        match provider_group.command {
            ProviderCommand::Login => {
                self.handle_provider_login().await?;
            }
            ProviderCommand::Logout => {
                self.handle_provider_logout().await?;
            }
            ProviderCommand::List => {
                self.on_show_providers(provider_group.porcelain).await?;
            }
        }

        Ok(())
    }

    async fn handle_provider_login(&mut self) -> anyhow::Result<()> {
        use crate::model::CliProvider;

        // Fetch all providers (configured and unconfigured)
        let providers = self
            .api
            .get_providers()
            .await?
            .into_iter()
            .map(CliProvider)
            .collect::<Vec<_>>();

        // Sort the providers by their display names
        let mut sorted_providers = providers;
        sorted_providers.sort_by_key(|a| a.to_string());

        // Use the centralized select module
        match ForgeSelect::select("Select a provider to login:", sorted_providers)
            .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
            .prompt()?
        {
            Some(provider) => {
                // Handle only unconfigured providers
                match provider.0 {
                    AnyProvider::Template(p) => {
                        let provider_id = p.id;
                        let auth_methods = p.auth_methods;

                        // Configure the provider
                        self.configure_provider(provider_id, auth_methods).await?;
                    }
                    AnyProvider::Url(provider) => {
                        self.configure_provider(provider.id, provider.auth_methods)
                            .await?;
                    }
                }
            }
            None => {
                self.writeln_title(TitleFormat::info("Cancelled"))?;
            }
        }

        Ok(())
    }

    async fn handle_provider_logout(&mut self) -> anyhow::Result<()> {
        use crate::model::CliProvider;

        // Fetch all providers
        let providers = self.api.get_providers().await?;

        // Filter only configured providers
        let configured_providers: Vec<_> = providers
            .into_iter()
            .filter_map(|p| match p {
                AnyProvider::Url(provider) => Some(CliProvider(AnyProvider::Url(provider))),
                AnyProvider::Template(_) => None,
            })
            .collect();

        if configured_providers.is_empty() {
            self.writeln_title(TitleFormat::info("No configured providers found"))?;
            return Ok(());
        }

        // Sort the providers by their display names
        let mut sorted_providers = configured_providers;
        sorted_providers.sort_by_key(|a| a.to_string());

        // Use the centralized select module
        match ForgeSelect::select("Select a provider to logout:", sorted_providers)
            .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
            .prompt()?
        {
            Some(provider) => {
                if let AnyProvider::Url(p) = provider.0 {
                    self.api.remove_provider(&p.id).await?;
                    self.writeln_title(TitleFormat::completion(format!(
                        "Successfully logged out from {}",
                        p.id
                    )))?;
                }
            }
            None => {
                self.writeln_title(TitleFormat::info("Cancelled"))?;
            }
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
            info = info.add_key_value(id, title);
        }

        if porcelain {
            let porcelain = Porcelain::from(&info).into_long().skip(1).drop_col(0);
            self.writeln(porcelain)?;
        } else {
            self.writeln(info)?;
        }

        Ok(())
    }

    /// Lists all the providers
    async fn on_show_providers(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let providers = self.api.get_providers().await?;

        if providers.is_empty() {
            return Ok(());
        }

        let mut info = Info::new();

        for provider in providers.iter() {
            let id = provider.id().to_string();
            let (domain, configured) = match provider {
                AnyProvider::Url(p) => (
                    p.url.domain().map(|d| d.to_string()).unwrap_or_default(),
                    true,
                ),
                AnyProvider::Template(_) => ("<unset>".to_string(), false),
            };
            info = info
                .add_title(id.to_case(Case::UpperSnake))
                .add_key_value("id", id)
                .add_key_value("host", domain);
            if configured {
                info = info.add_key_value("status", "available");
            };
        }

        if porcelain {
            let porcelain = Porcelain::from(&info).skip(1).drop_col(0);
            self.writeln(porcelain)?;
        } else {
            self.writeln(info)?;
        }

        Ok(())
    }

    /// Lists all the models
    async fn on_show_models(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let models = self.get_models().await?;

        if models.is_empty() {
            return Ok(());
        }

        let mut info = Info::new();

        for model in models.iter() {
            let id = model.id.to_string();

            info = info
                .add_title(model.name.as_ref().unwrap_or(&id))
                .add_key_value("Id", id);

            // Add context length if available, otherwise use "unknown"
            if let Some(limit) = model.context_length {
                let context = if limit >= 1_000_000 {
                    format!("{}M", limit / 1_000_000)
                } else if limit >= 1000 {
                    format!("{}k", limit / 1000)
                } else {
                    format!("{limit}")
                };
                info = info.add_key_value("Context Window", context);
            } else {
                info = info.add_key_value("Context Window", "<unavailable>")
            }

            // Add tools support indicator if explicitly supported
            if let Some(supported) = model.tools_supported {
                info = info.add_key_value(
                    "Tools",
                    if supported {
                        "Supported"
                    } else {
                        "Unsupported"
                    },
                )
            } else {
                info = info.add_key_value("Tools", "<unknown>")
            }
        }

        if porcelain {
            self.writeln(
                Porcelain::from(&info)
                    .skip(1)
                    .swap_cols(0, 1)
                    .map_col(3, |col| {
                        if col == Some("Supported".to_owned()) {
                            Some("🛠️".into())
                        } else {
                            None
                        }
                    }),
            )?;
        } else {
            self.writeln(info)?;
        }

        Ok(())
    }

    /// Lists all the commands
    async fn on_show_commands(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let mut info = Info::new();

        // Define base commands with their descriptions and aliases
        info = info
            .add_title("info")
            .add_key_value("type", "command")
            .add_key_value("description", "Print session information [alias: i]")
            .add_title("env")
            .add_key_value("type", "command")
            .add_key_value("description", "Display environment information [alias: e]")
            .add_title("provider")
            .add_key_value("type", "command")
            .add_key_value("description", "Switch the providers [alias: p]")
            .add_title("model")
            .add_key_value("type", "command")
            .add_key_value("description", "Switch the models [alias: m]")
            .add_title("new")
            .add_key_value("type", "command")
            .add_key_value("description", "Start new conversation [alias: n]")
            .add_title("dump")
            .add_key_value("type", "command")
            .add_key_value(
                "description",
                "Save conversation as JSON or HTML (use /dump html for HTML format) [alias: d]",
            )
            .add_title("conversation")
            .add_key_value("type", "command")
            .add_key_value(
                "description",
                "List all conversations for the active workspace [alias: c]",
            )
            .add_title("retry")
            .add_key_value("type", "command")
            .add_key_value("description", "Retry the last command [alias: r]")
            .add_title("compact")
            .add_key_value("type", "command")
            .add_key_value("description", "Compact the conversation context")
            .add_title("tools")
            .add_key_value("type", "command")
            .add_key_value(
                "description",
                "List all available tools with their descriptions and schema [alias: t]",
            )
            .add_title("suggest")
            .add_key_value("type", "command")
            .add_key_value(
                "description",
                "Generate shell commands without executing them [alias: s]",
            );

        // Add agent aliases
        info = info
            .add_title("ask")
            .add_key_value("type", "agent")
            .add_key_value(
                "description",
                "Research and investigation agent [alias for: sage]",
            )
            .add_title("plan")
            .add_key_value("type", "agent")
            .add_key_value(
                "description",
                "Planning and strategy agent [alias for: muse]",
            );

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
                .add_key_value("type", "agent")
                .add_key_value("description", title);
        }

        if porcelain {
            let porcelain = Porcelain::from(&info).swap_cols(1, 2).skip(1);
            self.writeln(porcelain)?;
        } else {
            self.writeln(info)?;
        }

        Ok(())
    }

    /// Lists current configuration values
    async fn on_show_config(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let model = self
            .get_agent_model(None)
            .await
            .map(|m| m.as_str().to_string());
        let model = model.unwrap_or_else(|| "<not set>".to_string());
        let provider = self
            .get_provider(None)
            .await
            .ok()
            .map(|p| p.id.to_string())
            .unwrap_or_else(|| "<not set>".to_string());

        let info = Info::new()
            .add_title("CONFIGURATION")
            .add_key_value("Default Model", model)
            .add_key_value("Default Provider", provider);

        if porcelain {
            self.writeln(Porcelain::from(&info).into_long().skip(1).drop_col(0))?;
        } else {
            self.writeln(info)?;
        }
        Ok(())
    }

    /// Displays available tools for the current agent
    async fn on_show_tools(&mut self, agent_id: AgentId, porcelain: bool) -> anyhow::Result<()> {
        self.spinner.start(Some("Loading"))?;
        let all_tools = self.api.get_tools().await?;
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
        if porcelain {
            self.writeln(Porcelain::from(&info).into_long().drop_col(1).skip(1))?;
        } else {
            self.writeln(info)?;
        }

        Ok(())
    }

    /// Displays all MCP servers
    async fn on_show_mcp_servers(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let mcp_servers = self.api.read_mcp_config(None).await?;

        let mut info = Info::new();

        for (name, server) in mcp_servers.mcp_servers {
            info = info
                .add_title(name.to_uppercase())
                .add_key_value("Command", server.to_string());

            if server.is_disabled() {
                info = info.add_key_value("Status", "disabled")
            }
        }

        if porcelain {
            self.writeln(Porcelain::from(&info).skip(1))?;
        } else {
            self.writeln(info)?;
        }

        Ok(())
    }

    async fn on_info(
        &mut self,
        porcelain: bool,
        conversation_id: Option<ConversationId>,
    ) -> anyhow::Result<()> {
        let mut info = Info::new();

        // Fetch conversation
        let conversation = match conversation_id {
            Some(conversation_id) => self.api.conversation(&conversation_id).await.ok().flatten(),
            None => None,
        };

        let key_info = self.api.get_login_info().await;
        // Fetch agent
        let agent = self.api.get_active_agent().await;

        // Fetch model (resolved with default model if unset)
        let model = self.get_agent_model(agent.clone()).await;

        // Fetch agent-specific provider or default provider if unset
        let agent_provider = self.get_provider(agent.clone()).await.ok();

        // Fetch default provider (could be different from the set provider)
        let default_provider = self.api.get_default_provider().await.ok();

        // Add agent information
        info = info.add_title("AGENT");
        if let Some(agent) = agent {
            info = info.add_key_value("ID", agent.as_str().to_uppercase());
        }

        // Add model information if available
        if let Some(model) = model {
            info = info.add_key_value("Model", model);
        }

        // Add provider information
        match (default_provider, agent_provider) {
            (Some(default), Some(agent_specific)) if default.id != agent_specific.id => {
                // Show both providers if they're different
                info = info.add_key_value("Agent Provider (URL)", &agent_specific.url);
                if let Some(api_key) = agent_specific.api_key() {
                    info = info.add_key_value("Agent API Key", truncate_key(api_key.as_str()));
                }

                info = info.add_key_value("Default Provider (URL)", &default.url);
                if let Some(api_key) = default.api_key() {
                    info = info.add_key_value("Default API Key", truncate_key(api_key.as_str()));
                }
            }
            (Some(provider), _) | (_, Some(provider)) => {
                // Show single provider (either default or agent-specific)
                info = info.add_key_value("Provider (URL)", &provider.url);
                if let Some(api_key) = provider.api_key() {
                    info = info.add_key_value("API Key", truncate_key(api_key.as_str()));
                }
            }
            _ => {
                // No provider available
            }
        }

        // Add user information if available
        if let Some(login_info) = key_info? {
            info = info.extend(Info::from(&login_info));
        }

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

        if porcelain {
            self.writeln(Porcelain::from(&info).into_long().skip(1))?;
        } else {
            self.writeln(info)?;
        }

        Ok(())
    }

    async fn on_env(&mut self) -> anyhow::Result<()> {
        let env = self.api.environment();
        let info = Info::from(&env);
        self.writeln(info)?;
        Ok(())
    }

    async fn on_zsh_prompt(&self) -> anyhow::Result<()> {
        println!("{}", include_str!("../../../shell-plugin/forge.plugin.zsh"));
        Ok(())
    }

    /// Handle the cmd command - generates shell command from natural language
    async fn on_cmd(&mut self, prompt: UserPrompt) -> anyhow::Result<()> {
        self.spinner.start(Some("Generating"))?;

        match self.api.generate_command(prompt).await {
            Ok(command) => {
                self.spinner.stop(None)?;
                self.writeln(command)?;
                Ok(())
            }
            Err(err) => {
                self.spinner.stop(None)?;
                Err(err)
            }
        }
    }

    async fn list_conversations(&mut self) -> anyhow::Result<()> {
        self.spinner.start(Some("Loading Conversations"))?;
        let max_conversations = self.api.environment().max_conversations;
        let conversations = self.api.get_conversations(Some(max_conversations)).await?;
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
            let conversation_id = conversation.id;
            self.state.conversation_id = Some(conversation_id);

            // Show conversation content
            self.on_show_last_message(conversation).await?;

            // Print log about conversation switching
            self.writeln_title(TitleFormat::info(format!(
                "Switched to conversation {}",
                conversation_id.into_string().bold()
            )))?;

            // Show conversation info
            self.on_info(false, Some(conversation_id)).await?;
        }
        Ok(())
    }

    async fn on_show_conversations(&mut self, porcelain: bool) -> anyhow::Result<()> {
        let max_conversations = self.api.environment().max_conversations;
        let conversations = self.api.get_conversations(Some(max_conversations)).await?;

        if conversations.is_empty() {
            return Ok(());
        }

        let mut info = Info::new().add_title("SESSIONS");

        for conv in conversations.into_iter() {
            if conv.context.is_none() {
                continue;
            }

            let title = conv
                .title
                .as_deref()
                .map(|t| t.to_string())
                .unwrap_or_else(|| format!("<untitled> [{}]", conv.id));

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
                .add_title(conv.id)
                .add_key_value("Title", title)
                .add_key_value("Updated", time_ago);
        }

        // In porcelain mode, skip the top-level "SESSIONS" title
        if porcelain {
            let porcelain = Porcelain::from(&info).skip(2).drop_col(3).truncate(1, 60);
            self.writeln(porcelain)?;
        } else {
            self.writeln(info)?;
        }

        Ok(())
    }

    async fn on_command(&mut self, command: SlashCommand) -> anyhow::Result<bool> {
        match command {
            SlashCommand::Conversations => {
                self.list_conversations().await?;
            }
            SlashCommand::Compact => {
                self.spinner.start(Some("Compacting"))?;
                self.on_compaction().await?;
            }
            SlashCommand::Dump(format) => {
                self.spinner.start(Some("Dumping"))?;
                self.on_dump(format).await?;
            }
            SlashCommand::New => {
                self.on_new().await?;
            }
            SlashCommand::Info => {
                self.on_info(false, None).await?;
            }
            SlashCommand::Env => {
                self.on_env().await?;
            }
            SlashCommand::Usage => {
                self.on_usage().await?;
            }
            SlashCommand::Message(ref content) => {
                self.spinner.start(None)?;
                self.on_message(Some(content.clone())).await?;
            }
            SlashCommand::Forge => {
                self.on_agent_change(AgentId::FORGE).await?;
            }
            SlashCommand::Muse => {
                self.on_agent_change(AgentId::MUSE).await?;
            }
            SlashCommand::Sage => {
                self.on_agent_change(AgentId::SAGE).await?;
            }
            SlashCommand::Help => {
                let info = Info::from(self.command.as_ref());
                self.writeln(info)?;
            }
            SlashCommand::Tools => {
                let agent_id = self.api.get_active_agent().await.unwrap_or_default();
                self.on_show_tools(agent_id, false).await?;
            }
            SlashCommand::Update => {
                on_update(self.api.clone(), None).await;
            }
            SlashCommand::Exit => {
                return Ok(true);
            }

            SlashCommand::Custom(event) => {
                self.spinner.start(None)?;
                self.on_custom_event(event.into()).await?;
            }
            SlashCommand::Model => {
                self.on_model_selection().await?;
            }
            SlashCommand::Provider => {
                self.on_provider_selection().await?;
            }
            SlashCommand::Shell(ref command) => {
                self.api.execute_shell_command_raw(command).await?;
            }
            SlashCommand::Agent => {
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
            SlashCommand::Login => {
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
            SlashCommand::Logout => {
                self.spinner.start(Some("Logging out"))?;
                self.api.logout().await?;
                self.spinner.stop(None)?;
                self.writeln_title(TitleFormat::info("Logged out"))?;
                // Exit the UI after logout
                return Ok(true);
            }
            SlashCommand::Retry => {
                self.spinner.start(None)?;
                self.on_message(None).await?;
            }
            SlashCommand::AgentSwitch(agent_id) => {
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
        let current_model = self
            .get_agent_model(self.api.get_active_agent().await)
            .await;
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
    async fn handle_api_key_input(
        &mut self,
        provider_id: ProviderId,
        request: &ApiKeyRequest,
    ) -> anyhow::Result<()> {
        use anyhow::Context;
        self.spinner.stop(None)?;
        // Collect URL parameters if required
        let url_params = request
            .required_params
            .iter()
            .map(|param| {
                let param_value = ForgeSelect::input(format!("Enter {}:", param))
                    .prompt()?
                    .context("Parameter input cancelled")?;

                anyhow::ensure!(!param_value.trim().is_empty(), "{} cannot be empty", param);

                Ok((param.to_string(), param_value))
            })
            .collect::<anyhow::Result<HashMap<_, _>>>()?;

        let api_key = ForgeSelect::input(format!("Enter your {} API key:", provider_id))
            .prompt()?
            .context("API key input cancelled")?;

        let api_key_str = api_key.trim();
        anyhow::ensure!(!api_key_str.is_empty(), "API key cannot be empty");

        // Update the context with collected data
        let response = AuthContextResponse::api_key(request.clone(), api_key_str, url_params);

        self.api
            .complete_provider_auth(
                provider_id,
                response,
                Duration::from_secs(0), // No timeout needed since we have the data
            )
            .await?;

        self.display_credential_success(provider_id).await?;

        Ok(())
    }

    fn display_oauth_device_info_new(
        &mut self,
        user_code: &str,
        verification_uri: &str,
        verification_uri_complete: Option<&str>,
    ) -> anyhow::Result<()> {
        use colored::Colorize;

        let display_uri = verification_uri_complete.unwrap_or(verification_uri);

        self.writeln("")?;
        self.writeln(format!(
            "{} Please visit: {}",
            "→".blue(),
            display_uri.blue().underline()
        ))?;
        self.writeln(format!(
            "{} Enter code: {}",
            "→".blue(),
            user_code.bold().yellow()
        ))?;
        self.writeln("")?;

        // Try to open browser automatically
        if let Err(e) = open::that(display_uri) {
            self.writeln_title(TitleFormat::error(format!(
                "Failed to open browser automatically: {}",
                e
            )))?;
        }

        Ok(())
    }

    async fn handle_device_flow(
        &mut self,
        provider_id: ProviderId,
        request: &DeviceCodeRequest,
    ) -> Result<()> {
        use std::time::Duration;

        let user_code = request.user_code.clone();
        let verification_uri = request.verification_uri.clone();
        let verification_uri_complete = request.verification_uri_complete.clone();

        self.spinner.stop(None)?;
        // Display OAuth device information
        self.display_oauth_device_info_new(
            user_code.as_ref(),
            verification_uri.as_ref(),
            verification_uri_complete.as_ref().map(|v| v.as_ref()),
        )?;

        // Step 2: Complete authentication (polls if needed for OAuth flows)
        self.spinner.start(Some("Completing authentication..."))?;

        let response = AuthContextResponse::device_code(request.clone());

        self.api
            .complete_provider_auth(provider_id, response, Duration::from_secs(600))
            .await?;

        self.spinner.stop(None)?;

        self.display_credential_success(provider_id).await?;
        Ok(())
    }

    async fn display_credential_success(&mut self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.writeln_title(TitleFormat::info(format!(
            "{} configured successfully!",
            provider_id
        )))?;

        // Prompt user to set as active provider
        let should_set_active = ForgeSelect::confirm(format!(
            "Would you like to set {} as the active provider?",
            provider_id
        ))
        .with_default(true)
        .prompt()?;

        if should_set_active.unwrap_or(false) {
            self.api.set_default_provider(provider_id).await?;
        }
        Ok(())
    }

    async fn handle_code_flow(
        &mut self,
        provider_id: ProviderId,
        request: &CodeRequest,
    ) -> anyhow::Result<()> {
        use colored::Colorize;

        self.spinner.stop(None)?;

        self.writeln(format!(
            "{}",
            format!("Authenticate using your {} account", provider_id).dimmed()
        ))?;

        // Display authorization URL
        self.writeln(format!(
            "{} Please visit: {}",
            "→".blue(),
            request.authorization_url.as_str().blue().underline()
        ))?;

        // Try to open browser automatically
        if let Err(e) = open::that(request.authorization_url.as_str()) {
            self.writeln_title(TitleFormat::error(format!(
                "Failed to open browser automatically: {}",
                e
            )))?;
        }

        // Prompt user to paste authorization code
        let code = ForgeSelect::input("Paste the authorization code:")
            .prompt()?
            .ok_or_else(|| anyhow::anyhow!("Authorization code input cancelled"))?;

        if code.trim().is_empty() {
            anyhow::bail!("Authorization code cannot be empty");
        }

        self.spinner
            .start(Some("Exchanging authorization code..."))?;

        let response = AuthContextResponse::code(request.clone(), &code);

        self.api
            .complete_provider_auth(
                provider_id,
                response,
                Duration::from_secs(0), // No timeout needed since we have the data
            )
            .await?;

        self.spinner.stop(None)?;

        self.display_credential_success(provider_id).await?;
        Ok(())
    }

    /// Helper method to select an authentication method when multiple are
    /// available
    async fn select_auth_method(
        &mut self,
        provider_id: ProviderId,
        auth_methods: &[AuthMethod],
    ) -> Result<Option<AuthMethod>> {
        use colored::Colorize;

        if auth_methods.is_empty() {
            anyhow::bail!(
                "No authentication methods available for provider {}",
                provider_id
            );
        }

        // If only one auth method, use it directly
        if auth_methods.len() == 1 {
            return Ok(Some(auth_methods[0].clone()));
        }

        // Multiple auth methods - ask user to choose
        self.spinner.stop(None)?;

        self.writeln_title(TitleFormat::action(format!("Configure {}", provider_id)))?;
        self.writeln("Multiple authentication methods available".dimmed())?;

        let method_names: Vec<String> = auth_methods
            .iter()
            .map(|method| match method {
                AuthMethod::ApiKey => "API Key".to_string(),
                AuthMethod::OAuthDevice(_) => "OAuth Device Flow".to_string(),
                AuthMethod::OAuthCode(_) => "OAuth Authorization Code".to_string(),
            })
            .collect();

        match ForgeSelect::select("Select authentication method:", method_names.clone())
            .with_help_message("Use arrow keys to navigate and Enter to select")
            .prompt()?
        {
            Some(selected_name) => {
                // Find the corresponding auth method
                let index = method_names
                    .iter()
                    .position(|name| name == &selected_name)
                    .expect("Selected method should exist");
                Ok(Some(auth_methods[index].clone()))
            }
            None => Ok(None),
        }
    }

    /// Handle authentication flow for an unavailable provider
    async fn configure_provider(
        &mut self,
        provider_id: ProviderId,
        auth_methods: Vec<AuthMethod>,
    ) -> Result<()> {
        // Select auth method (or use the only one available)
        let auth_method = match self.select_auth_method(provider_id, &auth_methods).await? {
            Some(method) => method,
            None => return Ok(()), // User cancelled
        };

        self.spinner.start(Some("Initiating authentication..."))?;

        // Initiate the authentication flow
        let auth_request = self
            .api
            .init_provider_auth(provider_id, auth_method)
            .await?;

        // Handle the specific authentication flow based on the request type
        match auth_request {
            AuthContextRequest::ApiKey(request) => {
                self.handle_api_key_input(provider_id, &request).await?;
            }
            AuthContextRequest::DeviceCode(request) => {
                self.handle_device_flow(provider_id, &request).await?;
            }
            AuthContextRequest::Code(request) => {
                self.handle_code_flow(provider_id, &request).await?;
            }
        }

        Ok(())
    }

    async fn select_provider(&mut self) -> Result<Option<Provider<Url>>> {
        // Fetch available providers
        let mut providers = self
            .api
            .get_providers()
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
        let current_provider = self
            .get_provider(self.api.get_active_agent().await)
            .await
            .ok();
        let starting_cursor = current_provider
            .as_ref()
            .and_then(|current| providers.iter().position(|p| p.0.id() == current.id))
            .unwrap_or(0);

        // Use the centralized select module
        match ForgeSelect::select("Select a provider:", providers)
            .with_starting_cursor(starting_cursor)
            .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
            .prompt()?
        {
            Some(provider) => {
                // Handle both configured and unconfigured providers
                match provider.0 {
                    AnyProvider::Url(p) => Ok(Some(p)),
                    AnyProvider::Template(p) => {
                        // Provider is not configured - initiate authentication flow
                        let provider_id = p.id;
                        let auth_methods = p.auth_methods;

                        // Configure the provider
                        self.configure_provider(provider_id, auth_methods).await?;

                        // After configuration, fetch the provider again
                        let providers = self.api.get_providers().await?;
                        let configured_provider = providers
                            .into_iter()
                            .find(|entry| entry.id() == provider_id)
                            .and_then(|entry| match entry {
                                AnyProvider::Url(p) => Some(p),
                                AnyProvider::Template(_) => None,
                            });

                        Ok(configured_provider)
                    }
                }
            }
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

        let active_agent = self.api.get_active_agent().await;

        // Update the operating model via API
        self.api
            .set_default_model(active_agent, model.clone())
            .await?;

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
        self.api.set_default_provider(provider.id).await?;

        self.writeln_title(TitleFormat::action(format!(
            "Switched to provider: {}",
            CliProvider(AnyProvider::Url(provider.clone()))
        )))?;

        // Check if the current model is available for the new provider
        let current_model = self
            .get_agent_model(self.api.get_active_agent().await)
            .await;
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
        let event: UserCommand = serde_json::from_str(&json)?;

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
        // Set agent if provided via CLI
        if let Some(agent_id) = self.cli.agent.clone() {
            self.api.set_active_agent(agent_id).await?;
        }

        let mut is_new = false;
        let id = if let Some(id) = self.state.conversation_id {
            id
        } else if let Some(ref id_str) = self.cli.conversation_id {
            // Parse and use the provided conversation ID
            let id = ConversationId::parse(id_str).context("Failed to parse conversation ID")?;

            // Check if conversation exists, if not create it
            if self.api.conversation(&id).await?.is_none() {
                let conversation = Conversation::new(id);
                self.api.upsert_conversation(conversation).await?;
                is_new = true;
            }
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
            is_new = true;
            self.api.upsert_conversation(conversation).await?;
            id
        };

        // Print if the state is being reinitialized
        if self.state.conversation_id.is_none() {
            self.print_conversation_status(is_new, id).await?;
        }

        // Always set the conversation id in state
        self.state.conversation_id = Some(id);

        Ok(id)
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

        if let Some(ref agent) = self.api.get_active_agent().await {
            sub_title.push_str(format!("via {}", agent).as_str());
        }

        if let Some(ref model) = self
            .get_agent_model(self.api.get_active_agent().await)
            .await
        {
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
        if self
            .get_agent_model(self.api.get_active_agent().await)
            .await
            .is_none()
        {
            let active_agent = self.api.get_active_agent().await;
            let model = self
                .select_model()
                .await?
                .ok_or(anyhow::anyhow!("Model selection is required to continue"))?;
            self.api.set_default_model(active_agent, model).await?;
        }

        // Create base workflow and trigger updates if this is the first initialization
        let mut base_workflow = Workflow::default();
        base_workflow.merge(workflow.clone());
        if first {
            // only call on_update if this is the first initialization
            on_update(self.api.clone(), base_workflow.updates.as_ref()).await;
            if !workflow.commands.is_empty() {
                self.writeln_title(TitleFormat::error("forge.yaml commands are deprecated. Use .md files in forge/ (home) or .forge/ (project) instead"))?;
            }
        }

        // Execute independent operations in parallel to improve performance
        let write_workflow_fut = self
            .api
            .write_workflow(self.cli.workflow.as_deref(), &workflow);
        let get_agents_fut = self.api.get_agents();
        let get_operating_agent_fut = self.api.get_active_agent();

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

        // Register all the commands
        self.command.register_all(self.api.get_commands().await?);

        let operating_model = self
            .get_agent_model(self.api.get_active_agent().await)
            .await;
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
        let operating_agent = self.api.get_active_agent().await.unwrap_or_default();
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
                if let Some(conversation_id) = self.state.conversation_id
                    && let Ok(conversation) =
                        self.validate_conversation_exists(&conversation_id).await
                {
                    self.on_show_conv_info(conversation).await?;
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

    async fn on_show_conv_info(&mut self, conversation: Conversation) -> anyhow::Result<()> {
        if !should_show_completion_prompt() {
            return Ok(());
        }

        self.spinner.start(Some("Loading Summary"))?;

        let info = Info::default().extend(&conversation);

        self.writeln(info)?;

        self.spinner.stop(None)?;

        // Only prompt for new conversation if in interactive mode and prompt is enabled
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

    /// Clones a conversation with a new ID
    ///
    /// # Arguments
    /// * `original` - The conversation to clone
    /// * `porcelain` - If true, output only the new conversation ID
    async fn on_clone_conversation(
        &mut self,
        original: Conversation,
        porcelain: bool,
    ) -> anyhow::Result<()> {
        // Create a new conversation with a new ID but same content
        let new_id = ConversationId::generate();
        let mut cloned = original.clone();
        cloned.id = new_id;

        // Upsert the cloned conversation
        self.api.upsert_conversation(cloned.clone()).await?;

        // Output based on format
        if porcelain {
            println!("{}", new_id);
        } else {
            self.writeln_title(
                TitleFormat::info("Cloned").sub_title(format!("[{} → {}]", original.id, cloned.id)),
            )?;
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

    /// Handle config command
    async fn handle_config_command(
        &mut self,
        command: crate::cli::ConfigCommand,
        porcelain: bool,
    ) -> Result<()> {
        match command {
            crate::cli::ConfigCommand::Set(args) => self.handle_config_set(args).await?,
            crate::cli::ConfigCommand::Get(args) => self.handle_config_get(args).await?,
            crate::cli::ConfigCommand::List => {
                self.on_show_config(porcelain).await?;
            }
        }
        Ok(())
    }

    /// Handle config set command
    async fn handle_config_set(&mut self, args: crate::cli::ConfigSetArgs) -> Result<()> {
        use crate::cli::ConfigField;

        // Set the specified field
        match args.field {
            ConfigField::Provider => {
                let provider_id = self.validate_provider(&args.value).await?;
                self.api.set_default_provider(provider_id).await?;
                self.writeln_title(TitleFormat::action("Provider set").sub_title(&args.value))?;
            }
            ConfigField::Model => {
                let model_id = self.validate_model(&args.value).await?;
                let active_agent = self.api.get_active_agent().await;
                self.api
                    .set_default_model(active_agent, model_id.clone())
                    .await?;
                self.writeln_title(
                    TitleFormat::action(model_id.as_str()).sub_title("is now the default model"),
                )?;
            }
        }

        Ok(())
    }

    /// Handle config get command
    async fn handle_config_get(&mut self, args: crate::cli::ConfigGetArgs) -> Result<()> {
        use crate::cli::ConfigField;

        // Get specific field
        match args.field {
            ConfigField::Model => {
                let model = self
                    .api
                    .get_default_model()
                    .await
                    .map(|m| m.as_str().to_string());
                match model {
                    Some(v) => self.writeln(v.to_string())?,
                    None => self.writeln("Model: Not set")?,
                }
            }
            ConfigField::Provider => {
                let provider = self
                    .api
                    .get_default_provider()
                    .await
                    .ok()
                    .map(|p| p.id.to_string());
                match provider {
                    Some(v) => self.writeln(v.to_string())?,
                    None => self.writeln("Provider: Not set")?,
                }
            }
        }

        Ok(())
    }

    /// Validate model exists
    async fn validate_model(&self, model_str: &str) -> Result<ModelId> {
        let models = self.api.get_models().await?;
        let model_id = ModelId::new(model_str);

        if models.iter().any(|m| m.id == model_id) {
            Ok(model_id)
        } else {
            // Show first 10 models as suggestions
            let available: Vec<_> = models.iter().take(10).map(|m| m.id.as_str()).collect();
            let suggestion = if models.len() > 10 {
                format!("{} (and {} more)", available.join(", "), models.len() - 10)
            } else {
                available.join(", ")
            };

            Err(anyhow::anyhow!(
                "Model '{}' not found. Available models: {}",
                model_str,
                suggestion
            ))
        }
    }

    /// Validate provider exists and has API key
    async fn validate_provider(&mut self, provider_str: &str) -> Result<ProviderId> {
        // Parse provider ID from string
        let provider_id = ProviderId::from_str(provider_str).with_context(|| {
            format!(
                "Invalid provider: '{}'. Valid providers are: {}",
                provider_str,
                get_valid_provider_names().join(", ")
            )
        })?;

        // Check if provider is configured
        let providers = self.api.get_providers().await?;
        let provider_entry = providers
            .iter()
            .find(|p| p.id() == provider_id)
            .ok_or_else(|| forge_domain::Error::provider_not_available(provider_id))?;

        if provider_entry.is_configured() {
            return Ok(provider_id);
        }

        match provider_entry {
            AnyProvider::Template(p) => {
                let auth_methods = p.auth_methods.clone();

                // Configure the provider
                self.configure_provider(provider_id, auth_methods).await?;

                // Verify configuration succeeded
                let providers = self.api.get_providers().await?;
                if providers
                    .iter()
                    .any(|p| p.id() == provider_id && p.is_configured())
                {
                    Ok(provider_id)
                } else {
                    Err(anyhow::anyhow!(
                        "Failed to configure provider {}",
                        provider_id
                    ))
                }
            }
            AnyProvider::Url(_) => Ok(provider_id),
        }
    }

    /// Shows the last message from a conversation
    ///
    /// # Errors
    /// - If the conversation doesn't exist
    /// - If the conversation has no messages
    async fn on_show_last_message(&mut self, conversation: Conversation) -> Result<()> {
        let context = conversation
            .context
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Conversation has no context"))?;

        // Find the last assistant message
        let message = context.messages.iter().rev().find_map(|msg| match msg {
            ContextMessage::Text(TextMessage { content, role: Role::Assistant, .. }) => {
                Some(content)
            }
            _ => None,
        });

        // Format and display the message using the message_display module
        if let Some(message) = message {
            self.writeln(self.markdown.render(message))?;
        }

        Ok(())
    }
}

/// Get list of valid provider names
fn get_valid_provider_names() -> Vec<String> {
    ProviderId::iter().map(|p| p.to_string()).collect()
}
