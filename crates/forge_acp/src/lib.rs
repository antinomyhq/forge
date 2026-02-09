mod agent;
mod conversion;
mod error;
mod server;

pub use agent::ForgeAgent;
pub use conversion::*;
pub use error::{Error, Result};
pub use server::{AgentInfo, agent_info, start_http_server, start_stdio_server};

pub(crate) const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};
