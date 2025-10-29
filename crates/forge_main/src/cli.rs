use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use forge_domain::AgentId;

#[derive(Parser)]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    /// Direct prompt to process without entering interactive mode.
    ///
    /// Allows running a single command directly from the command line.
    /// Alternatively, you can pipe content to forge: `cat prompt.txt | forge`
    #[arg(long, short = 'p')]
    pub prompt: Option<String>,

    /// Path to a file containing initial commands to execute.
    ///
    /// The application will execute the commands from this file first,
    /// then continue in interactive mode.
    #[arg(long, short = 'c')]
    pub command: Option<String>,

    /// Path to a file containing the conversation to execute.
    /// This file should be in JSON format.
    #[arg(long)]
    pub conversation: Option<PathBuf>,

    /// Conversation ID to use for this session.
    ///
    /// If provided, the application will use this conversation ID instead of
    /// generating a new one. This allows resuming or continuing existing
    /// conversations.
    #[arg(long, alias = "cid")]
    pub conversation_id: Option<String>,

    /// Working directory to set before starting forge.
    ///
    /// If provided, the application will change to this directory before
    /// starting. This allows running forge from a different directory.
    #[arg(long, short = 'C')]
    pub directory: Option<PathBuf>,

    /// Create isolated git worktree for experimentation
    #[arg(long)]
    pub sandbox: Option<String>,

    /// Enable verbose output mode.
    #[arg(long, default_value_t = false)]
    pub verbose: bool,

    /// Use restricted shell (rbash) for enhanced security
    #[arg(long, default_value_t = false, short = 'r')]
    pub restricted: bool,

    /// Top-level subcommands
    #[command(subcommand)]
    pub subcommands: Option<TopLevelCommand>,

    /// Path to a file containing the workflow to execute.
    #[arg(long, short = 'w')]
    pub workflow: Option<PathBuf>,

    /// Dispatch an event to the workflow.
    /// For example: --event '{"name": "fix_issue", "value": "449"}'
    #[arg(long, short = 'e')]
    pub event: Option<String>,
}

impl Cli {
    /// Checks if user is in is_interactive
    pub fn is_interactive(&self) -> bool {
        self.prompt.is_none() && self.command.is_none() && self.subcommands.is_none()
    }
}

#[derive(Subcommand, Debug, Clone)]
pub enum TopLevelCommand {
    /// Generate shell extension scripts
    #[command(hide = true)]
    Extension(ExtensionCommandGroup),

    /// List resources (agents, models, providers, commands, tools, mcp)
    List(ListCommandGroup),

    /// Display the banner with version and helpful information
    ///
    /// Example: forge banner
    Banner,

    /// Show current configuration, active model, and environment status
    Info {
        /// Optional conversation ID to show info for a specific session
        #[arg(long, alias = "cid")]
        conversation_id: Option<String>,

        /// Output in machine-readable format (porcelain)
        #[arg(long)]
        porcelain: bool,
    },

    /// Configuration management commands
    Config(ConfigCommandGroup),

    /// Session management commands (dump, retry, resume, list)
    Session(SessionCommandGroup),

    /// MCP server management commands
    Mcp(McpCommandGroup),
}

/// Group of list-related commands
#[derive(Parser, Debug, Clone)]
pub struct ListCommandGroup {
    #[command(subcommand)]
    pub command: ListCommand,

    /// Output in machine-readable format (porcelain)
    #[arg(long, global = true)]
    pub porcelain: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ListCommand {
    /// List all available agents
    ///
    /// Example: forge list agents
    Agents,

    /// List all available providers
    ///
    /// Example: forge list providers
    Providers,

    /// List all available models
    ///
    /// Example: forge list models
    Models,

    /// List all available commands
    ///
    /// Example: forge list commands
    #[command(hide = true)]
    Commands,

    /// List current configuration values
    ///
    /// Example: forge list config
    Config,

    /// List all tools for a specific agent
    ///
    /// Example: forge list tools sage
    Tools {
        /// Agent ID to show tools for
        agent: AgentId,
    },
    /// List all MCP servers
    ///
    /// Example: forge list mcp
    Mcp,

    /// List all conversations (sessions)
    ///
    /// Example: forge list session
    Session,
}

/// Group of extension-related commands
#[derive(Parser, Debug, Clone)]
pub struct ExtensionCommandGroup {
    #[command(subcommand)]
    pub command: ExtensionCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ExtensionCommand {
    /// Generate ZSH extension script
    Zsh,
}

/// Group of MCP-related commands
#[derive(Parser, Debug, Clone)]
pub struct McpCommandGroup {
    /// Subcommands under `mcp`
    #[command(subcommand)]
    pub command: McpCommand,

    /// Output in machine-readable format
    #[arg(long, global = true)]
    pub porcelain: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum McpCommand {
    /// Import MCP servers configuration from JSON
    Import(McpImportArgs),

    /// List servers
    List,

    /// Remove a server
    Remove(McpRemoveArgs),

    /// Show detailed configuration for a server
    Show(McpShowArgs),

    /// Reload MCP servers and rebuild caches
    Reload,
}

#[derive(Parser, Debug, Clone)]
pub struct McpImportArgs {
    /// The JSON configuration to import
    #[arg()]
    pub json: String,

    /// Configuration scope (local or user)
    #[arg(short = 's', long = "scope", default_value = "local")]
    pub scope: Scope,
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
pub struct McpShowArgs {
    /// Name of the server to show details for
    pub name: String,
}

/// Configuration scope (local, user, or project)
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

    /// Output in machine-readable format (tab-separated key-value pairs)
    #[arg(long, global = true)]
    pub porcelain: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommand {
    /// Set configuration values
    Set(ConfigSetArgs),

    /// Get a specific configuration value
    Get(ConfigGetArgs),

    /// List all configuration values
    List,
}

#[derive(Parser, Debug, Clone)]
pub struct ConfigSetArgs {
    /// Config field to set
    pub field: ConfigField,

    /// Value to set
    pub value: String,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    Agent,
    Model,
    Provider,
}

#[derive(Parser, Debug, Clone)]
pub struct ConfigGetArgs {
    /// Specific config field to get
    pub field: ConfigField,
}

/// Group of Session-related commands
#[derive(Parser, Debug, Clone)]
pub struct SessionCommandGroup {
    #[command(subcommand)]
    pub command: SessionCommand,

    /// Output in machine-readable format
    #[arg(long, global = true)]
    pub porcelain: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SessionCommand {
    /// List all conversations
    ///
    /// Example: forge session list
    List,

    /// Create a new conversation
    ///
    /// Example: forge session new
    New,

    /// Dump conversation as JSON or HTML
    ///
    /// Example: forge session dump abc123 html
    Dump {
        /// Conversation ID
        id: String,

        /// Output format: "html" for HTML, omit for JSON (default)
        format: Option<String>,
    },

    /// Compact the conversation context
    ///
    /// Example: forge session compact abc123
    Compact {
        /// Conversation ID
        id: String,
    },

    /// Retry the last command without modifying context
    ///
    /// Example: forge session retry abc123
    Retry {
        /// Conversation ID
        id: String,
    },

    /// Resume a conversation
    ///
    /// Example: forge session resume abc123
    Resume {
        /// Conversation ID
        id: String,
    },

    /// Show the last assistant message from a conversation
    ///
    /// Example: forge session show abc123
    Show {
        /// Conversation ID
        id: String,
    },

    /// Show detailed information about a session
    ///
    /// Example: forge session info abc123
    Info {
        /// Conversation ID
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_config_set_with_agent() {
        let fixture = Cli::parse_from(["forge", "config", "set", "agent", "muse"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Set(args) if args.field == ConfigField::Agent => {
                    Some(args.value.clone())
                }
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
            "model",
            "anthropic/claude-sonnet-4",
        ]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Set(args) if args.field == ConfigField::Model => {
                    Some(args.value.clone())
                }
                _ => None,
            },
            _ => None,
        };
        let expected = Some("anthropic/claude-sonnet-4".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_set_with_provider() {
        let fixture = Cli::parse_from(["forge", "config", "set", "provider", "OpenAI"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Set(args) if args.field == ConfigField::Provider => {
                    Some(args.value.clone())
                }
                _ => None,
            },
            _ => None,
        };
        let expected = Some("OpenAI".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_list() {
        let fixture = Cli::parse_from(["forge", "config", "list"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => matches!(config.command, ConfigCommand::List),
            _ => false,
        };
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_get_specific_field() {
        let fixture = Cli::parse_from(["forge", "config", "get", "model"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => match config.command {
                ConfigCommand::Get(args) => args.field,
                _ => panic!("Expected ConfigCommand::Get"),
            },
            _ => panic!("Expected TopLevelCommand::Config"),
        };
        let expected = ConfigField::Model;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_list() {
        let fixture = Cli::parse_from(["forge", "session", "list"]);
        let is_list = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => {
                matches!(session.command, SessionCommand::List)
            }
            _ => false,
        };
        assert_eq!(is_list, true);
    }

    #[test]
    fn test_session_dump_json_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "dump", "abc123"]);
        let (id, format) = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => match session.command {
                SessionCommand::Dump { id, format } => (id, format),
                _ => (String::new(), None),
            },
            _ => (String::new(), None),
        };
        assert_eq!(id, "abc123");
        assert_eq!(format, None); // JSON is default
    }

    #[test]
    fn test_session_dump_html_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "dump", "abc123", "html"]);
        let (id, format) = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => match session.command {
                SessionCommand::Dump { id, format } => (id, format),
                _ => (String::new(), None),
            },
            _ => (String::new(), None),
        };
        assert_eq!(id, "abc123");
        assert_eq!(format, Some("html".to_string()));
    }

    #[test]
    fn test_session_retry_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "retry", "xyz789"]);
        let id = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => match session.command {
                SessionCommand::Retry { id } => id,
                _ => String::new(),
            },
            _ => String::new(),
        };
        assert_eq!(id, "xyz789");
    }

    #[test]
    fn test_session_compact_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "compact", "abc123"]);
        let id = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => match session.command {
                SessionCommand::Compact { id } => id,
                _ => String::new(),
            },
            _ => String::new(),
        };
        assert_eq!(id, "abc123");
    }

    #[test]
    fn test_session_last_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "show", "test123"]);
        let id = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => match session.command {
                SessionCommand::Show { id } => id,
                _ => String::new(),
            },
            _ => String::new(),
        };
        assert_eq!(id, "test123");
    }

    #[test]
    fn test_session_resume() {
        let fixture = Cli::parse_from(["forge", "session", "resume", "def456"]);
        let id = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => match session.command {
                SessionCommand::Resume { id } => id,
                _ => String::new(),
            },
            _ => String::new(),
        };
        assert_eq!(id, "def456");
    }

    #[test]
    fn test_list_tools_command_with_agent() {
        let fixture = Cli::parse_from(["forge", "list", "tools", "sage"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::List(list)) => match list.command {
                ListCommand::Tools { agent } => agent,
                _ => AgentId::default(),
            },
            _ => AgentId::default(),
        };
        let expected = AgentId::new("sage");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_list_session_command() {
        let fixture = Cli::parse_from(["forge", "list", "session"]);
        let is_session_list = match fixture.subcommands {
            Some(TopLevelCommand::List(list)) => matches!(list.command, ListCommand::Session),
            _ => false,
        };
        assert_eq!(is_session_list, true);
    }

    #[test]
    fn test_info_command_without_porcelain() {
        let fixture = Cli::parse_from(["forge", "info"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Info { porcelain, .. }) => porcelain,
            _ => true,
        };
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_info_command_with_porcelain() {
        let fixture = Cli::parse_from(["forge", "info", "--porcelain"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Info { porcelain, .. }) => porcelain,
            _ => false,
        };
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_info_command_with_conversation_id() {
        let fixture = Cli::parse_from(["forge", "info", "--conversation-id", "abc123"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Info { conversation_id, .. }) => conversation_id,
            _ => None,
        };
        let expected = Some("abc123".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_info_command_with_cid_alias() {
        let fixture = Cli::parse_from(["forge", "info", "--cid", "xyz789"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Info { conversation_id, .. }) => conversation_id,
            _ => None,
        };
        let expected = Some("xyz789".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_info_command_with_conversation_id_and_porcelain() {
        let fixture = Cli::parse_from(["forge", "info", "--cid", "test123", "--porcelain"]);
        let (conversation_id, porcelain) = match fixture.subcommands {
            Some(TopLevelCommand::Info { conversation_id, porcelain }) => {
                (conversation_id, porcelain)
            }
            _ => (None, false),
        };
        assert_eq!(conversation_id, Some("test123".to_string()));
        assert_eq!(porcelain, true);
    }

    #[test]
    fn test_list_agents_without_porcelain() {
        let fixture = Cli::parse_from(["forge", "list", "agents"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::List(list)) => list.porcelain,
            _ => true,
        };
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_list_agents_with_porcelain() {
        let fixture = Cli::parse_from(["forge", "list", "agents", "--porcelain"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::List(list)) => list.porcelain,
            _ => false,
        };
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mcp_list_with_porcelain() {
        let fixture = Cli::parse_from(["forge", "mcp", "list", "--porcelain"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Mcp(mcp)) => mcp.porcelain,
            _ => false,
        };
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_list_with_porcelain() {
        let fixture = Cli::parse_from(["forge", "session", "list", "--porcelain"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => session.porcelain,
            _ => false,
        };
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_list_models_with_porcelain() {
        let fixture = Cli::parse_from(["forge", "list", "models", "--porcelain"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::List(list)) => list.porcelain,
            _ => false,
        };
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_list_with_porcelain() {
        let fixture = Cli::parse_from(["forge", "config", "list", "--porcelain"]);
        let actual = match fixture.subcommands {
            Some(TopLevelCommand::Config(config)) => config.porcelain,
            _ => false,
        };
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_info_with_id() {
        let fixture = Cli::parse_from(["forge", "session", "info", "abc123"]);
        let id = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => match session.command {
                SessionCommand::Info { id } => id,
                _ => String::new(),
            },
            _ => String::new(),
        };
        assert_eq!(id, "abc123");
    }

    #[test]
    fn test_session_info_with_porcelain() {
        let fixture = Cli::parse_from(["forge", "session", "info", "test123", "--porcelain"]);
        let (id, porcelain) = match fixture.subcommands {
            Some(TopLevelCommand::Session(session)) => match session.command {
                SessionCommand::Info { id } => (id, session.porcelain),
                _ => (String::new(), false),
            },
            _ => (String::new(), false),
        };
        assert_eq!(id, "test123");
        assert_eq!(porcelain, true);
    }

    #[test]
    fn test_prompt_with_conversation_id() {
        let fixture = Cli::parse_from([
            "forge",
            "-p",
            "hello",
            "--conversation-id",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        let actual = fixture.conversation_id;
        let expected = Some("550e8400-e29b-41d4-a716-446655440000".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_conversation_id_without_prompt() {
        let fixture = Cli::parse_from([
            "forge",
            "--conversation-id",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        let actual = fixture.conversation_id;
        let expected = Some("550e8400-e29b-41d4-a716-446655440000".to_string());
        assert_eq!(actual, expected);
    }
}
