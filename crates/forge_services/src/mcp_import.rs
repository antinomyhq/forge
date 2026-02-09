/// Service for importing MCP servers from external sources
///
/// Handles conversion of MCP servers from external formats (IDE clients, etc.)
/// into Forge's internal configuration format. This service manages the conversion
/// and merging logic, delegating storage to the McpConfigManager infrastructure.
use anyhow::Result;
use forge_app::{ExternalMcpServer, McpConfigManager, McpImportService};
use forge_domain::{McpConfig, McpServerConfig, Scope, ServerName};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Imports MCP servers from external sources into Forge configuration
///
/// This service converts external MCP server definitions (from IDEs like Cursor,
/// Windsurf, etc.) into Forge's internal format and merges them with existing
/// configuration.
pub struct ForgeMcpImportService<M> {
    config_manager: Arc<M>,
}

impl<M> ForgeMcpImportService<M> {
    /// Creates a new MCP import service
    ///
    /// # Arguments
    ///
    /// * `config_manager` - Infrastructure providing McpConfigManager
    pub fn new(config_manager: Arc<M>) -> Self {
        Self { config_manager }
    }
}

#[async_trait::async_trait]
impl<M: McpConfigManager> McpImportService for ForgeMcpImportService<M> {
    async fn import_servers(
        &self,
        servers: Vec<ExternalMcpServer>,
        scope: &Scope,
    ) -> Result<()> {
        // Convert all external servers to Forge format
        let converted: Vec<(String, McpServerConfig)> = servers
            .iter()
            .map(|server| self.convert_server(server))
            .collect::<Result<Vec<_>>>()?;

        // Merge with existing configuration
        self.merge_with_existing(converted, scope).await
    }

    fn convert_server(&self, server: &ExternalMcpServer) -> Result<(String, McpServerConfig)> {
        match server {
            ExternalMcpServer::Stdio {
                name,
                command,
                args,
                env,
            } => {
                // Convert Vec<(String, String)> to BTreeMap<String, String>
                let env_map: BTreeMap<String, String> = env.iter().cloned().collect();

                let config = McpServerConfig::new_stdio(
                    command.clone(),
                    args.clone(),
                    Some(env_map),
                );
                Ok((name.clone(), config))
            }
            ExternalMcpServer::Http {
                name,
                url,
                headers,
            } => {
                let mut config = McpServerConfig::new_http(url);
                if let McpServerConfig::Http(ref mut http_config) = config {
                    // Convert Vec<(String, String)> to BTreeMap<String, String>
                    http_config.headers = headers.iter().cloned().collect();
                }
                Ok((name.clone(), config))
            }
            ExternalMcpServer::Sse {
                name,
                url,
                headers,
            } => {
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
        scope: &Scope,
    ) -> Result<()> {
        // Load existing configuration for this scope
        let mut existing_config = self
            .config_manager
            .read_mcp_config(Some(scope))
            .await
            .unwrap_or_else(|_| McpConfig {
                mcp_servers: BTreeMap::new(),
            });

        // Merge new servers (new servers overwrite existing ones with same name)
        for (name, config) in new_servers {
            existing_config.mcp_servers.insert(name.into(), config);
        }

        // Save merged configuration
        self.config_manager
            .write_mcp_config(&existing_config, scope)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::sync::Mutex;

    // Mock config manager for testing
    struct MockConfigManager {
        configs: Mutex<BTreeMap<Scope, McpConfig>>,
    }

    impl MockConfigManager {
        fn new() -> Self {
            Self {
                configs: Mutex::new(BTreeMap::new()),
            }
        }

        fn with_config(scope: Scope, config: McpConfig) -> Self {
            let mut configs = BTreeMap::new();
            configs.insert(scope, config);
            Self {
                configs: Mutex::new(configs),
            }
        }
    }

    #[async_trait::async_trait]
    impl McpConfigManager for MockConfigManager {
        async fn read_mcp_config(&self, scope: Option<&Scope>) -> Result<McpConfig> {
            let scope = scope.unwrap_or(&Scope::User);
            self.configs
                .lock()
                .unwrap()
                .get(scope)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Config not found"))
        }

        async fn write_mcp_config(&self, config: &McpConfig, scope: &Scope) -> Result<()> {
            self.configs
                .lock()
                .unwrap()
                .insert(*scope, config.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_convert_stdio_server() {
        let manager = Arc::new(MockConfigManager::new());
        let service = ForgeMcpImportService::new(manager);

        let external = ExternalMcpServer::Stdio {
            name: "test-server".to_string(),
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            env: vec![("KEY".to_string(), "value".to_string())],
        };

        let (name, config) = service.convert_server(&external).unwrap();

        assert_eq!(name, "test-server");
        match config {
            McpServerConfig::Stdio(stdio) => {
                assert_eq!(stdio.command, "node");
                assert_eq!(stdio.args, vec!["server.js"]);
                assert_eq!(stdio.env.get("KEY"), Some(&"value".to_string()));
            }
            _ => panic!("Expected Stdio config"),
        }
    }

    #[tokio::test]
    async fn test_convert_http_server() {
        let manager = Arc::new(MockConfigManager::new());
        let service = ForgeMcpImportService::new(manager);

        let external = ExternalMcpServer::Http {
            name: "http-server".to_string(),
            url: "http://localhost:8080".to_string(),
            headers: vec![("Authorization".to_string(), "Bearer token".to_string())],
        };

        let (name, config) = service.convert_server(&external).unwrap();

        assert_eq!(name, "http-server");
        match config {
            McpServerConfig::Http(http) => {
                assert_eq!(http.url, "http://localhost:8080");
                assert_eq!(
                    http.headers.get("Authorization"),
                    Some(&"Bearer token".to_string())
                );
            }
            _ => panic!("Expected Http config"),
        }
    }

    #[tokio::test]
    async fn test_convert_sse_server() {
        let manager = Arc::new(MockConfigManager::new());
        let service = ForgeMcpImportService::new(manager);

        let external = ExternalMcpServer::Sse {
            name: "sse-server".to_string(),
            url: "http://localhost:9090/events".to_string(),
            headers: vec![("X-Custom".to_string(), "header".to_string())],
        };

        let (name, config) = service.convert_server(&external).unwrap();

        assert_eq!(name, "sse-server");
        // SSE is converted to HTTP (auto-detected by URL)
        match config {
            McpServerConfig::Http(http) => {
                assert_eq!(http.url, "http://localhost:9090/events");
                assert_eq!(http.headers.get("X-Custom"), Some(&"header".to_string()));
            }
            _ => panic!("Expected Http config for SSE"),
        }
    }

    #[tokio::test]
    async fn test_import_servers_to_empty_config() {
        let manager = Arc::new(MockConfigManager::new());
        let service = ForgeMcpImportService::new(manager.clone());

        let servers = vec![ExternalMcpServer::Stdio {
            name: "new-server".to_string(),
            command: "python".to_string(),
            args: vec!["server.py".to_string()],
            env: vec![],
        }];

        let result = service.import_servers(servers, &Scope::User).await;

        assert!(result.is_ok());

        // Verify the server was saved
        let saved_config = manager.read_mcp_config(Some(&Scope::User)).await.unwrap();
        assert!(saved_config.mcp_servers.contains_key("new-server"));
    }

    #[tokio::test]
    async fn test_merge_with_existing_servers() {
        let mut existing_servers = BTreeMap::new();
        existing_servers.insert(
            ServerName::from("existing-server".to_string()),
            McpServerConfig::new_stdio("node", vec![], None),
        );

        let existing_config = McpConfig {
            mcp_servers: existing_servers,
        };

        let manager = Arc::new(MockConfigManager::with_config(Scope::User, existing_config));
        let service = ForgeMcpImportService::new(manager.clone());

        let servers = vec![ExternalMcpServer::Http {
            name: "new-http-server".to_string(),
            url: "http://example.com".to_string(),
            headers: vec![],
        }];

        let result = service.import_servers(servers, &Scope::User).await;

        assert!(result.is_ok());

        // Verify both servers exist
        let saved_config = manager.read_mcp_config(Some(&Scope::User)).await.unwrap();
        assert!(saved_config
            .mcp_servers
            .contains_key(&ServerName::from("existing-server".to_string())));
        assert!(saved_config
            .mcp_servers
            .contains_key(&ServerName::from("new-http-server".to_string())));
        assert_eq!(saved_config.mcp_servers.len(), 2);
    }

    #[tokio::test]
    async fn test_import_overwrites_existing_server_with_same_name() {
        let mut existing_servers = BTreeMap::new();
        existing_servers.insert(
            ServerName::from("duplicate".to_string()),
            McpServerConfig::new_stdio("old-command", vec![], None),
        );

        let existing_config = McpConfig {
            mcp_servers: existing_servers,
        };

        let manager = Arc::new(MockConfigManager::with_config(Scope::User, existing_config));
        let service = ForgeMcpImportService::new(manager.clone());

        let servers = vec![ExternalMcpServer::Stdio {
            name: "duplicate".to_string(),
            command: "new-command".to_string(),
            args: vec![],
            env: vec![],
        }];

        let result = service.import_servers(servers, &Scope::User).await;

        assert!(result.is_ok());

        // Verify the server was overwritten
        let saved_config = manager.read_mcp_config(Some(&Scope::User)).await.unwrap();
        let server = saved_config
            .mcp_servers
            .get(&ServerName::from("duplicate".to_string()))
            .unwrap();
        match server {
            McpServerConfig::Stdio(stdio) => {
                assert_eq!(stdio.command, "new-command");
            }
            _ => panic!("Expected Stdio config"),
        }
    }
}
