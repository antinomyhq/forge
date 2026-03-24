mod auto_dump;
mod config;
mod error;
mod http_config;
mod merge;
mod model_config;
mod reader;
mod retry_config;
mod writer;

pub use auto_dump::*;
pub use config::*;
pub use error::Error;
pub use http_config::*;
pub use model_config::*;
pub use retry_config::*;

/// A `Result` type alias for this crate's [`Error`] type.
pub type Result<T> = std::result::Result<T, Error>;
