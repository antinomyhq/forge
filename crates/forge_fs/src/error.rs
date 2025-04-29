use std::string::FromUtf8Error;

use thiserror::Error;

/// Error type for file operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("Binary files are not supported. File detected as {0}")]
    BinaryFileNotSupported(String),

    #[error("Start position {start} is beyond the file size of {total} characters")]
    StartBeyondFileSize { start: u64, total: u64 },

    #[error("Start position {start} is greater than end position {end}")]
    StartGreaterThanEnd { start: u64, end: u64 },

    #[error("UTF-8 validation failed: {0}")]
    Utf8ValidationFailed(#[from] FromUtf8Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
