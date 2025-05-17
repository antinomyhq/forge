use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use forge_domain::{EnvironmentService, McpConfig, McpConfigManager, Scope};
use merge::Merge;

use crate::{FsReadService, FsWriteService, Infrastructure};

pub struct ForgeMcpManager<I> {
    infra: Arc<I>,
}

impl<I: Infrastructure> ForgeMcpManager<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    async fn read_or_default_config(&self, path: &Path) -> anyhow::Result<McpConfig> {
        let config = self.infra.file_read_service().read_utf8(path).await;
        if let Ok(config) = config {
            serde_json::from_str(&config).context(format!("Invalid config at: {}", path.display()))
        } else {
            Ok(McpConfig::default())
        }
    }
    async fn config_path(&self, scope: &Scope) -> anyhow::Result<PathBuf> {
        let env = self.infra.environment_service().get_environment();
        match scope {
            Scope::User => Ok(env.mcp_user_config()),
            Scope::Local => Ok(env.mcp_local_config()),
        }
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> McpConfigManager for ForgeMcpManager<I> {
    async fn read(&self) -> anyhow::Result<McpConfig> {
        let env = self.infra.environment_service().get_environment();
        let user_config = env.mcp_user_config();
        let local_config = env.mcp_local_config();

        // NOTE: Config at lower levels has higher priority.

        let mut config = McpConfig::default();
        config.merge(self.read_or_default_config(&user_config).await?);
        config.merge(self.read_or_default_config(&local_config).await?);

        Ok(config)
    }

    async fn write(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()> {
        self.infra
            .file_write_service()
            .write(
                self.config_path(scope).await?.as_path(),
                Bytes::from(serde_json::to_string(config)?),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use bytes::Bytes;
    use forge_domain::{McpConfig, McpServerConfig, McpStdioServer, Scope};
    use merge::Merge;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::{FsMetaService, FsReadService, FsWriteService};

    // Helper function to create a test McpConfig with a single server
    fn create_test_config(name: &str) -> McpConfig {
        let mut servers = BTreeMap::new();
        servers.insert(
            name.to_string(),
            McpServerConfig::Stdio(McpStdioServer {
                command: name.to_string(),
                args: vec![],
                env: BTreeMap::new(),
            }),
        );
        McpConfig { mcp_servers: servers }
    }

    // Mock infrastructure with controlled environment paths
    struct TestMockInfrastructure {
        base: MockInfrastructure,
    }

    impl TestMockInfrastructure {
        fn new() -> Self {
            Self { base: MockInfrastructure::new() }
        }
    }

    impl Clone for TestMockInfrastructure {
        fn clone(&self) -> Self {
            Self { base: self.base.clone() }
        }
    }

    // Modified ForgeMcpManager with overridden config path methods for testing
    struct TestForgeMcpManager<I> {
        inner: ForgeMcpManager<I>,
        user_config_path: PathBuf,
        local_config_path: PathBuf,
    }

    impl<I: Infrastructure> TestForgeMcpManager<I> {
        fn new(infra: Arc<I>, user_config_path: PathBuf, local_config_path: PathBuf) -> Self {
            Self {
                inner: ForgeMcpManager::new(infra),
                user_config_path,
                local_config_path,
            }
        }

        async fn config_path(&self, scope: &Scope) -> anyhow::Result<PathBuf> {
            match scope {
                Scope::User => Ok(self.user_config_path.clone()),
                Scope::Local => Ok(self.local_config_path.clone()),
            }
        }
    }

    #[async_trait::async_trait]
    impl<I: Infrastructure> McpConfigManager for TestForgeMcpManager<I> {
        async fn read(&self) -> anyhow::Result<McpConfig> {
            let user_config = &self.user_config_path;
            let local_config = &self.local_config_path;

            let mut config = McpConfig::default();
            config.merge(self.inner.read_or_default_config(user_config).await?);
            config.merge(self.inner.read_or_default_config(local_config).await?);

            Ok(config)
        }

        async fn write(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()> {
            self.inner
                .infra
                .file_write_service()
                .write(
                    self.config_path(scope).await?.as_path(),
                    Bytes::from(serde_json::to_string(config)?),
                )
                .await
        }
    }

    // Implementation of Infrastructure for our test mock
    impl Infrastructure for TestMockInfrastructure {
        type EnvironmentService = <MockInfrastructure as Infrastructure>::EnvironmentService;
        type FsMetaService = <MockInfrastructure as Infrastructure>::FsMetaService;
        type FsReadService = <MockInfrastructure as Infrastructure>::FsReadService;
        type FsRemoveService = <MockInfrastructure as Infrastructure>::FsRemoveService;
        type FsSnapshotService = <MockInfrastructure as Infrastructure>::FsSnapshotService;
        type FsWriteService = <MockInfrastructure as Infrastructure>::FsWriteService;
        type FsCreateDirsService = <MockInfrastructure as Infrastructure>::FsCreateDirsService;
        type CommandExecutorService =
            <MockInfrastructure as Infrastructure>::CommandExecutorService;
        type InquireService = <MockInfrastructure as Infrastructure>::InquireService;
        type McpServer = <MockInfrastructure as Infrastructure>::McpServer;

        fn environment_service(&self) -> &Self::EnvironmentService {
            self.base.environment_service()
        }

        fn file_meta_service(&self) -> &Self::FsMetaService {
            self.base.file_meta_service()
        }

        fn file_read_service(&self) -> &Self::FsReadService {
            self.base.file_read_service()
        }

        fn file_remove_service(&self) -> &Self::FsRemoveService {
            self.base.file_remove_service()
        }

        fn file_snapshot_service(&self) -> &Self::FsSnapshotService {
            self.base.file_snapshot_service()
        }

        fn file_write_service(&self) -> &Self::FsWriteService {
            self.base.file_write_service()
        }

        fn create_dirs_service(&self) -> &Self::FsCreateDirsService {
            self.base.create_dirs_service()
        }

        fn command_executor_service(&self) -> &Self::CommandExecutorService {
            self.base.command_executor_service()
        }

        fn inquire_service(&self) -> &Self::InquireService {
            self.base.inquire_service()
        }

        fn mcp_server(&self) -> &Self::McpServer {
            self.base.mcp_server()
        }
    }

    #[tokio::test]
    async fn test_read_with_no_configs() {
        let user_config = PathBuf::from("/home/test/.config/forge/mcp.json");
        let local_config = PathBuf::from("/test/.forge/mcp.json");

        let infra = Arc::new(TestMockInfrastructure::new());
        let manager = TestForgeMcpManager::new(infra, user_config, local_config);

        let result = manager.read().await;

        assert!(result.is_ok(), "Read should succeed with empty configs");
        let config = result.unwrap();
        assert_eq!(
            config.mcp_servers.len(),
            0,
            "Default config should be empty"
        );
    }

    #[tokio::test]
    async fn test_read_with_user_config_only() {
        let user_config = PathBuf::from("/home/test/.config/forge/mcp.json");
        let local_config = PathBuf::from("/test/.forge/mcp.json");

        let infra = Arc::new(TestMockInfrastructure::new());
        let name = "user-server";

        // Add a user config
        let user_config_data = create_test_config(name);
        infra
            .base
            .file_write_service()
            .write(
                &user_config,
                Bytes::from(serde_json::to_string(&user_config_data).unwrap()),
            )
            .await
            .unwrap();

        let manager = TestForgeMcpManager::new(infra, user_config, local_config);

        let result = manager.read().await;

        let config = result.unwrap();
        assert_eq!(
            config.mcp_servers.len(),
            1,
            "Should have one server from user config"
        );

        let server = config.mcp_servers.get(name).unwrap();
        match server {
            McpServerConfig::Stdio(stdio) => {
                assert_eq!(stdio.command, name);
            }
            _ => panic!("Expected stdio"),
        }
    }

    #[tokio::test]
    async fn test_read_with_local_config_only() {
        let user_config = PathBuf::from("/home/test/.config/forge/mcp.json");
        let local_config = PathBuf::from("/test/.forge/mcp.json");
        let local_server = "local-server";

        let infra = Arc::new(TestMockInfrastructure::new());

        let local_config_data = create_test_config(local_server);
        infra
            .base
            .file_write_service()
            .write(
                &local_config,
                Bytes::from(serde_json::to_string(&local_config_data).unwrap()),
            )
            .await
            .unwrap();

        let manager = TestForgeMcpManager::new(infra, user_config, local_config);

        let result = manager.read().await;

        let config = result.unwrap();
        assert_eq!(
            config.mcp_servers.len(),
            1,
            "Should have one server from local config"
        );

        let server = &config.mcp_servers.get(local_server).unwrap();
        match server {
            McpServerConfig::Stdio(stdio) => {
                assert_eq!(stdio.command, "local-server");
            }
            _ => panic!("Expected stdio server"),
        }
    }

    #[tokio::test]
    async fn test_read_with_priority() {
        let user_config = PathBuf::from("/home/test/.config/forge/mcp.json");
        let local_config = PathBuf::from("/test/.forge/mcp.json");

        let infra = Arc::new(TestMockInfrastructure::new());
        let common_server = "common-server";

        let mut user_config_data = create_test_config(common_server);
        user_config_data.mcp_servers.insert(
            "user".to_string(),
            McpServerConfig::Stdio(McpStdioServer {
                command: "user".to_string(),
                args: vec![],
                env: BTreeMap::new(),
            }),
        );

        infra
            .base
            .file_write_service()
            .write(
                &user_config,
                Bytes::from(serde_json::to_string(&user_config_data).unwrap()),
            )
            .await
            .unwrap();

        let mut local_config_data = McpConfig::default();
        local_config_data.mcp_servers.insert(
            common_server.to_string(),
            McpServerConfig::Stdio(McpStdioServer {
                command: "override".to_string(), // This should override the user config
                args: vec![],
                env: BTreeMap::new(),
            }),
        );

        local_config_data.mcp_servers.insert(
            "local".to_string(),
            McpServerConfig::Stdio(McpStdioServer {
                command: "local".to_string(),
                args: vec![],
                env: BTreeMap::new(),
            }),
        );

        infra
            .base
            .file_write_service()
            .write(
                &local_config,
                Bytes::from(serde_json::to_string(&local_config_data).unwrap()),
            )
            .await
            .unwrap();

        let manager = TestForgeMcpManager::new(infra.clone(), user_config, local_config);

        // Execute
        let result = manager.read().await;

        let config = result.unwrap();

        let common_server = &config.mcp_servers.get(common_server).unwrap();
        match common_server {
            McpServerConfig::Stdio(stdio) => {
                assert_eq!(
                    stdio.command, "override",
                    "Local config should override user config"
                );
            }
            _ => panic!("Expected stdio server"),
        }

        // Both unique servers should be present
        assert!(
            config.mcp_servers.contains_key("user"),
            "Should contain user server"
        );
        assert!(
            config.mcp_servers.contains_key("local"),
            "Should contain local server"
        );
    }

    #[tokio::test]
    async fn test_read_with_invalid_user_config() {
        let user_config = PathBuf::from("/home/test/.config/forge/mcp.json");
        let local_config = PathBuf::from("/test/.forge/mcp.json");

        let infra = Arc::new(TestMockInfrastructure::new());

        let local_config_data = create_test_config("local-server");
        infra
            .base
            .file_write_service()
            .write(
                &local_config,
                Bytes::from(serde_json::to_string(&local_config_data).unwrap()),
            )
            .await
            .unwrap();

        infra
            .base
            .file_write_service()
            .write(&user_config, Bytes::from("invalid json"))
            .await
            .unwrap();

        let manager = TestForgeMcpManager::new(infra, user_config, local_config);

        let result = manager.read().await;
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid config"),
            "Error should mention invalid config: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_write_to_user_scope() {
        let user_config = PathBuf::from("/home/test/.config/forge/mcp.json");
        let local_config = PathBuf::from("/test/.forge/mcp.json");

        let infra = Arc::new(TestMockInfrastructure::new());
        let manager = TestForgeMcpManager::new(infra.clone(), user_config.clone(), local_config);

        let test_config = create_test_config("new-user-server");

        let result = manager.write(&test_config, &Scope::User).await;

        assert!(result.is_ok(), "Write should succeed");

        let meta_result = infra.file_meta_service().is_file(&user_config).await;
        assert!(
            meta_result.is_ok() && meta_result.unwrap(),
            "File should exist"
        );

        let content_result = infra.file_read_service().read_utf8(&user_config).await;
        assert!(content_result.is_ok(), "Should be able to read the file");

        let read_config: McpConfig = serde_json::from_str(&content_result.unwrap()).unwrap();
        assert!(
            read_config.mcp_servers.contains_key("new-user-server"),
            "Should contain the server we added"
        );
    }

    #[tokio::test]
    async fn test_write_to_local_scope() {
        let user_config = PathBuf::from("/home/test/.config/forge/mcp.json");
        let local_config = PathBuf::from("/test/.forge/mcp.json");

        let infra = Arc::new(TestMockInfrastructure::new());
        let manager = TestForgeMcpManager::new(infra.clone(), user_config, local_config.clone());

        let test_config = create_test_config("new-local-server");

        let result = manager.write(&test_config, &Scope::Local).await;

        assert!(result.is_ok(), "Write should succeed");

        let meta_result = infra.file_meta_service().is_file(&local_config).await;
        assert!(
            meta_result.is_ok() && meta_result.unwrap(),
            "File should exist"
        );

        let content_result = infra.file_read_service().read_utf8(&local_config).await;
        assert!(content_result.is_ok(), "Should be able to read the file");

        let read_config: McpConfig = serde_json::from_str(&content_result.unwrap()).unwrap();

        assert!(
            read_config.mcp_servers.contains_key("new-local-server"),
            "Should contain the server we added"
        );
    }
}
