use std::path::PathBuf;

/// Operations that can be performed and need policy checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Write operation to a file path
    Write { path: PathBuf, cwd: PathBuf },
    /// Read operation from a file path
    Read { path: PathBuf, cwd: PathBuf },
    /// Execute operation with a command string
    Execute { command: String, cwd: PathBuf },
    /// Network fetch operation with a URL
    Fetch { url: String, cwd: PathBuf },
}
