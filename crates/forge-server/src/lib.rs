mod app;
mod completion;
mod conversation;
mod error;
mod log;
mod server;

use std::convert::Infallible;

use axum::response::sse::Event;
pub use error::*;
pub use server::Server;
use tokio_stream::Stream;

type EventStream = Box<dyn Stream<Item = std::result::Result<Event, Infallible>> + Send + Unpin>;
