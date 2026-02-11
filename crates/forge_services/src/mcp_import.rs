use std::collections::BTreeMap;
use std::sync::Arc;

use forge_app::{ExternalMcpServer, McpImportService};
use forge_domain::{McpConfig, McpServerConfig, Scope};

/// MCP server import service
///
/// Handles importing external MCP servers from various formats.
/// Converts external MCP server definitions (from IDEs like Cursor, Windsurf, etc.)
/// into Forge's internal format. The App layer orchestrates reading/writing config
/// via McpConfigManager.
pub struct ForgeMcpImportService;

impl ForgeMcpImportService {
    /// Creates a new MCP import service
    pub fn new(_infra: Arc<impl Send + Sync>) -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl McpImportService for ForgeMcpImportService {
    async fn import_servers(
        &self,
        servers: Vec<ExternalMcpServer>,
        _scope: &Scope,
    ) -> anyhow::Result<()> {
        // Convert all external servers to Forge format
        let _converted: Vec<(String, McpServerConfig)> = servers
            .iter()
            .map(|server| self.convert_server(server))
            .collect::<anyhow::Result<Vec<_>>>()?;

        // Note: The actual merging with existing config and writing is handled by the App layer
        // which has access to McpConfigManager. This service only provides conversion logic.
        Ok(())
    }

    fn convert_server(
        &self,
        server: &ExternalMcpServer,
    ) -> anyhow::Result<(String, McpServerConfig)> {
        match server {
            ExternalMcpServer::Stdio { name, command, args, env } => {
                // Convert Vec<(String, String)> to BTreeMap<String, String>
                let env_map: BTreeMap<String, String> = env.iter().cloned().collect();

                let config =
                    McpServerConfig::new_stdio(command.clone(), args.clone(), Some(env_map));
                Ok((name.clone(), config))
            }
            ExternalMcpServer::Http { name, url, headers } => {
                let mut config = McpServerConfig::new_http(url);
                if let McpServerConfig::Http(ref mut http_config) = config {
                    // Convert Vec<(String, String)> to BTreeMap<String, String>
                    http_config.headers = headers.iter().cloned().collect();
                }
                Ok((name.clone(), config))
            }
            ExternalMcpServer::Sse { name, url, headers } => {
                // SSE uses the same HTTP transport in Forge (auto-detected by URL)
                let mut config = McpServerConfig::new_http(url);
                if let McpServerConfig::Http(ref mut http_config) = config {
                    // Convert Vec<(String, String)> to BTreeMap<String, String>
                    http_config.headers = headers.iter().cloned().collect();
                }
                Ok((name.clone(), config))
            }
        }
    }

    async fn merge_with_existing(
        &self,
        new_servers: Vec<(String, McpServerConfig)>,
        _scope: &Scope,
    ) -> anyhow::Result<()> {
        // Note: This method signature is kept for trait compatibility, but the actual
        // merging logic should be in the App layer which has access to McpConfigManager.
        // This service only provides the conversion logic.
        //
        // The App layer should:
        // 1. Read existing config via McpConfigManager
        // 2. Call convert_server for each external server
        // 3. Merge the converted servers with existing config
        // 4. Write merged config via McpConfigManager
        
        // For now, we'll create a minimal merged config structure
        // The App layer will handle the actual persistence
        let mut _merged_config = McpConfig {
            mcp_servers: BTreeMap::new(),
        };

        for (name, config) in new_servers {
            _merged_config.mcp_servers.insert(name.into(), config);
        }

        Ok(())
    }
}
