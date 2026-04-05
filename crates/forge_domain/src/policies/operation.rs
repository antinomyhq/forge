use std::path::{Path, PathBuf};

/// Operations that can be performed and need policy checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionOperation {
    /// Write operation to a file path
    Write {
        path: PathBuf,
        cwd: PathBuf,
        message: String,
    },
    /// Read operation from a file path
    Read {
        path: PathBuf,
        cwd: PathBuf,
        message: String,
    },
    /// Execute operation with a command string
    Execute { command: String, cwd: PathBuf },
    /// Network fetch operation with a URL
    Fetch {
        url: String,
        cwd: PathBuf,
        message: String,
    },
    /// MCP tool call operation
    Mcp {
        /// The name of the MCP server (e.g. "github", "filesystem")
        server_name: String,
        /// The name of the tool being called (e.g. "list_issues", "read_file")
        tool_name: String,
        /// The working directory at the time of the tool call
        cwd: PathBuf,
    },
}

impl PermissionOperation {
    /// Returns the working directory associated with this operation.
    pub fn cwd(&self) -> &Path {
        match self {
            PermissionOperation::Write { cwd, .. } => cwd,
            PermissionOperation::Read { cwd, .. } => cwd,
            PermissionOperation::Execute { cwd, .. } => cwd,
            PermissionOperation::Fetch { cwd, .. } => cwd,
            PermissionOperation::Mcp { cwd, .. } => cwd,
        }
    }
}
