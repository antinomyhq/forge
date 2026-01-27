//! Agent Client Protocol (ACP) integration for Forge.
//!
//! This crate implements the ACP protocol, allowing Forge to act as an ACP-compatible
//! agent that can be invoked from IDEs like Zed and JetBrains products.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐         ACP Protocol          ┌──────────────────┐
//! │  IDE (Client)   │ ◄──────────────────────────► │  Forge Agent     │
//! │  (Zed/JetBrains)│    JSON-RPC over stdio/HTTP   │  (ACP Server)    │
//! └─────────────────┘                               └──────────────────┘
//!                                                           │
//!                                                           │ Uses existing
//!                                                           ▼
//!                                                   ┌──────────────────┐
//!                                                   │  ForgeApp        │
//!                                                   │  Services        │
//!                                                   │  MCP Tools       │
//!                                                   └──────────────────┘
//! ```

mod agent;
mod error;
mod server;

pub use agent::ForgeAgent;
pub use error::{Error, Result};
pub use server::{start_http_server, start_stdio_server, AgentInfo};
