//! JSON-RPC server implementation for the Forge API.
//!
//! This crate provides a JSON-RPC 2.0 server that wraps the Forge API and
//! exposes its functionality over stdio, allowing external applications to
//! communicate with the Forge engine using the JSON-RPC protocol.

mod error;
mod methods;
mod serialization;
mod server;
mod stream_handler;

pub use error::*;
pub use server::*;
