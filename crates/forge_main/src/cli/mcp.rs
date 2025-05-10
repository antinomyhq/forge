use clap::{Parser, Subcommand, ValueEnum};

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
    pub name: Option<String>,

    /// URL or command for the MCP server
    pub command_or_url: Option<String>,

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

    /// JSON string containing the server configuration
    pub json: String,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Scope {
    Local,
    User,
    Project,
}

impl From<Scope> for forge_domain::Scope {
    fn from(value: Scope) -> Self {
        match value {
            Scope::Local => forge_domain::Scope::Local,
            Scope::User => forge_domain::Scope::User,
            Scope::Project => forge_domain::Scope::Project,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum Transport {
    Stdio,
    Sse,
}
