use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

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
