//! GCC (Git Context Controller) domain types and utilities.

mod branch;
mod commit;
mod context_level;

use anyhow::Result;
pub use branch::*;
pub use commit::*;
pub use context_level::*;
// Common error type for GCC operations
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GccError {
    #[error("Branch not found: {0}")]
    BranchNotFound(String),
    #[error("Commit not found: {0}")]
    CommitNotFound(String),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Other error: {0}")]
    Other(String),
}

pub type GccResult<T> = Result<T, GccError>;
