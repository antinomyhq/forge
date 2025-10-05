use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use forge_domain::AgentId;

#[derive(Parser)]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    /// Path to a file containing initial commands to execute.
    ///
    /// The application will execute the commands from this file first,
    /// then continue in interactive mode.
    #[arg(long, short = 'c')]
    pub command: Option<String>,

    /// Direct prompt to process without entering interactive mode.
    ///
    /// Allows running a single command directly from the command line.
    /// Alternatively, you can pipe content to forge: `cat prompt.txt | forge`
    #[arg(long, short = 'p')]
    pub prompt: Option<String>,

    /// Enable verbose output mode.
    ///
    /// When enabled, shows additional debugging information and tool execution
    /// details.
    #[arg(long, default_value_t = false)]
    pub verbose: bool,

    /// Enable restricted shell mode for enhanced security.
    ///
    /// Controls the shell execution environment:
    /// - Default (false): Uses standard shells (bash on Unix/Mac, cmd on
    ///   Windows)
    /// - Restricted (true): Uses restricted shell (rbash) with limited
    ///   capabilities
    ///
    /// The restricted mode provides additional security by preventing:
    /// - Changing directories
    /// - Setting/modifying environment variables
    /// - Executing commands with absolute paths
    /// - Modifying shell options
    #[arg(long, default_value_t = false, short = 'r')]
    pub restricted: bool,

    /// Path to a file containing the workflow to execute.
    #[arg(long, short = 'w')]
    pub workflow: Option<PathBuf>,

    /// Dispatch an event to the workflow.
    /// For example: --event '{"name": "fix_issue", "value": "449"}'
    #[arg(long, short = 'e')]
    pub event: Option<String>,

    /// Path to a file containing the conversation to execute.
    /// This file should be in JSON format.
    #[arg(long)]
    pub conversation: Option<PathBuf>,

    /// Generate a new conversation ID and exit.
    ///
    /// When enabled, generates a new unique conversation ID and prints it to
    /// stdout. This ID can be used with the FORGE_CONVERSATION_ID environment
    /// variable to manage multiple terminal sessions with separate conversation
    /// contexts.
    #[arg(long, default_value_t = false)]
    pub generate_conversation_id: bool,

    /// Top-level subcommands
    #[command(subcommand)]
    pub subcommands: Option<TopLevelCommand>,

    /// Working directory to set before starting forge.
    ///
    /// If provided, the application will change to this directory before
    /// starting. This allows running forge from a different directory.
    pub directory: Option<PathBuf>,
    /// Create a new sandbox env and start forge in that directory.
    ///
    /// When specified, creates a new git worktree in the parent folder
    /// (if it doesn't already exist) and then starts forge in that directory.
    /// The worktree name will be used as the branch name.
    #[arg(long)]
    pub sandbox: Option<String>,
}

impl Cli {
    /// Checks if user is in is_interactive
    pub fn is_interactive(&self) -> bool {
        self.prompt.is_none()
            && self.event.is_none()
            && self.command.is_none()
            && self.subcommands.is_none()
    }
}

#[derive(Subcommand, Debug, Clone)]
pub enum TopLevelCommand {
    Mcp(McpCommandGroup),
    /// Print information about the environment
    Info,
    /// Generate ZSH shell prompt completion scripts
    GenerateZSHPrompt,

    /// Lists all the agents
    ShowAgents,

    /// Lists all the providers
    ShowProviders,

    /// Lists all the models
    ShowModels,

    /// Lists all the commands
    ShowCommands,

    /// Lists all the tools for a specific agent
    ShowTools {
        /// Agent ID to show tools for
        agent: AgentId,
    },

    /// Display the banner with version and helpful information
    ShowBanner,

    /// Configuration management commands
    Config(ConfigCommandGroup),

    /// Session management commands (dump, retry, resume, list)
    Session(SessionCommandGroup),
}

/// Group of MCP-related commands
#[derive(Parser, Debug, Clone)]
pub struct McpCommandGroup {
    /// Subcommands under `mcp`
    #[command(subcommand)]
    pub command: McpCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum McpCommand {
    /// Add a server
    Add(McpAddArgs),

    /// List servers
    List,

    /// Remove a server
    Remove(McpRemoveArgs),

    /// Get server details
    Get(McpGetArgs),

    /// Add a server in JSON format
    AddJson(McpAddJsonArgs),

    /// Cache management commands
    Cache(McpCacheArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct McpAddArgs {
    /// Configuration scope (local, user, or project)
    #[arg(short = 's', long = "scope", default_value = "local")]
    pub scope: Scope,

    /// Transport type (stdio or sse)
    #[arg(short = 't', long = "transport", default_value = "stdio")]
    pub transport: Transport,

    /// Environment variables, e.g. -e KEY=value
    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    /// Name of the server
    pub name: String,

    /// URL or command for the MCP server
    pub command_or_url: String,

    /// Additional arguments to pass to the server
    #[arg(short = 'a', long = "args")]
    pub args: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct McpRemoveArgs {
    /// Configuration scope (local, user, or project)
    #[arg(short = 's', long = "scope", default_value = "local")]
    pub scope: Scope,

    /// Name of the server to remove
    pub name: String,
}

#[derive(Parser, Debug, Clone)]
pub struct McpGetArgs {
    /// Name of the server to get details for
    pub name: String,
}

#[derive(Parser, Debug, Clone)]
pub struct McpAddJsonArgs {
    /// Configuration scope (local, user, or project)
    #[arg(short = 's', long = "scope", default_value = "local")]
    pub scope: Scope,

    /// Name of the server
    pub name: String,

    /// JSON string containing the server configuration
    pub json: String,
}

#[derive(Parser, Debug, Clone)]
pub struct McpCacheArgs {
    /// Cache subcommand
    #[command(subcommand)]
    pub command: McpCacheCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum McpCacheCommand {
    /// Rebuild caches by fetching fresh data from MCPs
    Refresh,
}

#[derive(Copy, Clone, Debug, ValueEnum, Default)]
pub enum Scope {
    #[default]
    Local,
    User,
}

impl From<Scope> for forge_domain::Scope {
    fn from(value: Scope) -> Self {
        match value {
            Scope::Local => forge_domain::Scope::Local,
            Scope::User => forge_domain::Scope::User,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum Transport {
    Stdio,
    Sse,
}

/// Group of Config-related commands
#[derive(Parser, Debug, Clone)]
pub struct ConfigCommandGroup {
    /// Subcommands under `config`
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommand {
    /// Set configuration values
    Set(ConfigSetArgs),

    /// Get configuration values
    Get(ConfigGetArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ConfigSetArgs {
    /// Agent to set as active
    #[arg(long)]
    pub agent: Option<String>,

    /// Model to set as active
    #[arg(long)]
    pub model: Option<String>,

    /// Provider to set as active
    #[arg(long)]
    pub provider: Option<String>,
}

impl ConfigSetArgs {
    /// Check if any field is set (non-interactive mode)
    pub fn has_any_field(&self) -> bool {
        self.agent.is_some() || self.model.is_some() || self.provider.is_some()
    }
}

#[derive(Parser, Debug, Clone)]
pub struct ConfigGetArgs {
    /// Specific field to get (agent, model, or provider). If not specified,
    /// shows all.
    #[arg(long)]
    pub field: Option<String>,
}

/// Group of Session-related commands
#[derive(Parser, Debug, Clone)]
pub struct SessionCommandGroup {
    /// Session/conversation ID to operate on (required for dump, retry, and
    /// resume)
    #[arg(long, short = 'i', required_unless_present = "list")]
    pub id: Option<String>,

    /// Session subcommand
    #[command(subcommand)]
    pub command: Option<SessionCommand>,

    /// List all conversations (doesn't require --id)
    #[arg(long)]
    pub list: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SessionCommand {
    /// Dump conversation as JSON or HTML
    Dump(SessionDumpArgs),

    /// Compact the conversation context
    Compact,

    /// Retry the last command without modifying context
    Retry,
}

#[derive(Parser, Debug, Clone)]
pub struct SessionDumpArgs {
    /// Output format: "html" for HTML, omit for JSON (default)
    pub format: Option<String>,
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_config_set_with_agent() {
        let fixture = Cli::parse_from(["forge", "config", "set", "--agent", "muse"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Set(args) => args.agent,
                _ => None,
            },
            _ => None,
        };
        let expected = Some("muse".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_with_model() {
        let fixture = Cli::parse_from([
            "forge",
            "config",
            "set",
            "--model",
            "anthropic/claude-sonnet-4",
        ]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Set(args) => args.model,
                _ => None,
            },
            _ => None,
        };
        let expected = Some("anthropic/claude-sonnet-4".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_with_provider() {
        let fixture = Cli::parse_from(["forge", "config", "set", "--provider", "OpenAI"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Set(args) => args.provider,
                _ => None,
            },
            _ => None,
        };
        let expected = Some("OpenAI".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_with_multiple_fields() {
        let fixture = Cli::parse_from([
            "forge",
            "config",
            "set",
            "--agent",
            "sage",
            "--model",
            "gpt-4",
            "--provider",
            "OpenAI",
        ]);
        let (agent, model, provider) = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Set(args) => (args.agent, args.model, args.provider),
                _ => (None, None, None),
            },
            _ => (None, None, None),
        };
        assert_eq!(agent, Some("sage".to_string()));
        assert_eq!(model, Some("gpt-4".to_string()));
        assert_eq!(provider, Some("OpenAI".to_string()));
    }

    #[test]
    fn test_config_set_no_fields() {
        let fixture = Cli::parse_from(["forge", "config", "set"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Set(args) => args.has_any_field(),
                _ => true,
            },
            _ => true,
        };
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_get_all() {
        let fixture = Cli::parse_from(["forge", "config", "get"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Get(args) => args.field,
                _ => Some("invalid".to_string()),
            },
            _ => Some("invalid".to_string()),
        };
        let expected = None;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_get_specific_field() {
        let fixture = Cli::parse_from(["forge", "config", "get", "--field", "model"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Get(args) => args.field,
                _ => None,
            },
            _ => None,
        };
        let expected = Some("model".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_args_has_any_field_with_agent() {
        let fixture = ConfigSetArgs {
            agent: Some("forge".to_string()),
            model: None,
            provider: None,
        };
        let actual = fixture.has_any_field();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_args_has_any_field_with_model() {
        let fixture = ConfigSetArgs {
            agent: None,
            model: Some("gpt-4".to_string()),
            provider: None,
        };
        let actual = fixture.has_any_field();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_args_has_any_field_with_provider() {
        let fixture = ConfigSetArgs {
            agent: None,
            model: None,
            provider: Some("OpenAI".to_string()),
        };
        let actual = fixture.has_any_field();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_args_has_no_field() {
        let fixture = ConfigSetArgs { agent: None, model: None, provider: None };
        let actual = fixture.has_any_field();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_dump_json_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "--id", "abc123", "dump"]);
        let (id, format) = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => {
                let format = match session.command {
                    Some(SessionCommand::Dump(args)) => args.format,
                    _ => None,
                };
                (session.id, format)
            }
            _ => (None, None),
        };
        assert_eq!(id, Some("abc123".to_string()));
        assert_eq!(format, None); // JSON is default
    }

    #[test]
    fn test_session_dump_html_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "--id", "abc123", "dump", "html"]);
        let (id, format) = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => {
                let format = match session.command {
                    Some(SessionCommand::Dump(args)) => args.format,
                    _ => None,
                };
                (session.id, format)
            }
            _ => (None, None),
        };
        assert_eq!(id, Some("abc123".to_string()));
        assert_eq!(format, Some("html".to_string()));
    }

    #[test]
    fn test_session_retry_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "--id", "xyz789", "retry"]);
        let (id, is_retry) = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => {
                let is_retry = matches!(session.command, Some(SessionCommand::Retry));
                (session.id, is_retry)
            }
            _ => (None, false),
        };
        assert_eq!(id, Some("xyz789".to_string()));
        assert_eq!(is_retry, true);
    }

    #[test]
    fn test_session_list_no_id_required() {
        let fixture = Cli::parse_from(["forge", "session", "--list"]);
        let is_list = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => session.list,
            _ => false,
        };
        assert_eq!(is_list, true);
    }

    #[test]
    fn test_session_resume_with_id_no_subcommand() {
        let fixture = Cli::parse_from(["forge", "session", "--id", "def456"]);
        let (id, has_subcommand) = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => {
                let has_subcommand = session.command.is_some();
                (session.id, has_subcommand)
            }
            _ => (None, true),
        };
        assert_eq!(id, Some("def456".to_string()));
        assert_eq!(has_subcommand, false); // No subcommand means resume
    }

    #[test]
    fn test_session_compact_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "--id", "abc123", "compact"]);
        let (id, command) = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => {
                let command = match session.command {
                    Some(SessionCommand::Compact) => "compact",
                    _ => "other",
                };
                (session.id, command)
            }
            _ => (None, "none"),
        };
        assert_eq!(id, Some("abc123".to_string()));
        assert_eq!(command, "compact");
    }

    #[test]
    fn test_session_dump_without_id_fails() {
        // This should fail because --id is required
        let result = Cli::try_parse_from(["forge", "session", "dump"]);
        assert!(result.is_err(), "Expected error when --id is not provided");
    }

    #[test]
    fn test_session_retry_without_id_fails() {
        // This should fail because --id is required
        let result = Cli::try_parse_from(["forge", "session", "retry"]);
        assert!(result.is_err(), "Expected error when --id is not provided");
    }

    #[test]
    fn test_show_tools_command_with_agent() {
        let fixture = Cli::parse_from(["forge", "show-tools", "sage"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::ShowTools { agent }) => agent,
            _ => AgentId::default(),
        };
        let expected = AgentId::new("sage");
        assert_eq!(actual, expected);
    }
}
