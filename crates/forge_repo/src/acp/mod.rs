//! Agent Client Protocol (ACP) integration for Forge.
//!
//! This module provides ACP server functionality, allowing Forge to be used
//! as an AI coding agent in ACP-compatible IDEs like Zed and JetBrains IDEs.

mod agent;
mod error;
mod server;

pub use agent::ForgeAgent;
pub use error::{Error, Result};
pub use server::{agent_info, start_http_server, start_stdio_server, AgentInfo};
