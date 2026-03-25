mod auto_dump;
mod compact;
mod config;
mod error;
mod http;
mod model;
mod reader;
mod retry;
mod writer;

pub use auto_dump::*;
pub use compact::*;
pub use config::*;
pub use error::Error;
pub use http::*;
pub use model::*;
pub use reader::ConfigReader;
pub use retry::*;
pub use writer::ConfigWriter;

/// A `Result` type alias for this crate's [`Error`] type.
pub type Result<T> = std::result::Result<T, Error>;
