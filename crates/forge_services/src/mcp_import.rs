use std::sync::Arc;

use forge_app::{ExternalMcpServer, McpImportService};
use forge_domain::{McpServerConfig, Scope};

/// MCP server import service
///
/// Handles importing external MCP servers from various formats.
pub struct ForgeMcpImportService<F> {
    _infra: Arc<F>,
}

impl<F> ForgeMcpImportService<F> {
    /// Creates a new MCP import service
    pub fn new(infra: Arc<F>) -> Self {
        Self { _infra: infra }
    }
}

#[async_trait::async_trait]
impl<F: Send + Sync> McpImportService for ForgeMcpImportService<F> {
    async fn import_servers(
        &self,
        _servers: Vec<ExternalMcpServer>,
        _scope: &Scope,
    ) -> anyhow::Result<()> {
        anyhow::bail!("McpImportService not yet implemented")
    }

    fn convert_server(
        &self,
        _server: &ExternalMcpServer,
    ) -> anyhow::Result<(String, McpServerConfig)> {
        anyhow::bail!("McpImportService not yet implemented")
    }

    async fn merge_with_existing(
        &self,
        _new_servers: Vec<(String, McpServerConfig)>,
        _scope: &Scope,
    ) -> anyhow::Result<()> {
        anyhow::bail!("McpImportService not yet implemented")
    }
}
