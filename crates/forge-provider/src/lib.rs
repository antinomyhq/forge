mod error;
mod log;
mod model;
mod ollama;
#[allow(unused)]
mod open_ai;
mod open_router;
mod provider;
mod test_server;

pub use error::*;
pub use model::*;
pub use provider::*;

pub type Stream<A> = Box<dyn tokio_stream::Stream<Item = A> + Unpin>;
