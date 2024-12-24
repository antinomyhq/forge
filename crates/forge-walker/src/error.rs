use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Blocking error")]
    JoinError(tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, Error>;
