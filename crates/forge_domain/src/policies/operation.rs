use std::path::PathBuf;

/// Operations that can be performed and need policy checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Write operation to a file path
    Write { path: PathBuf },
    /// Read operation from a file path
    Read { path: PathBuf },
    /// Patch operation to modify a file path
    Patch { path: PathBuf },
    /// Execute operation with a command string
    Execute { command: String },
}
