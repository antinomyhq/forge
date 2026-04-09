pub mod stdio;

use async_trait::async_trait;
use jsonrpsee::server::Server;

/// Transport trait for JSON-RPC server
#[async_trait]
pub trait Transport: Send + Sync {
    /// Run the transport with the given server
    async fn run(&self, server: Server) -> anyhow::Result<()>;
}
