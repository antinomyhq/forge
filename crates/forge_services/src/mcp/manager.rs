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
    use std::sync::Arc;

    use bytes::Bytes;
    use forge_domain::{McpConfig, McpServerConfig, McpSseServer, McpStdioServer, Scope};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::attachment::tests::{MockEnvironmentService, MockInfrastructure};

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

    // Test fixture with controlled paths
    struct TestFixture {
        infra: Arc<MockInfrastructure>,
    }

    impl TestFixture {
        fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();

            // temp dir acts as home dir, and temp/local dir acts as cwd.
            let user_dir = temp_dir.path().to_path_buf();
            let local_dir = user_dir.join("local");
            std::fs::create_dir(&local_dir).unwrap();

            // Create the infrastructure with mocked services
            let mut infra = MockInfrastructure::new();
            infra.env_service =
                Arc::new(MockEnvironmentService { home: Some(user_dir), cwd: Some(local_dir) });
            let infra = Arc::new(infra);
            infra.environment_service().get_environment();

            Self { infra }
        }

        fn manager(&self) -> ForgeMcpManager<MockInfrastructure> {
            ForgeMcpManager::new(self.infra.clone())
        }

        // Write raw content to a specific location (for invalid config tests)
        async fn write_raw(&self, path: &Path, content: &str) -> anyhow::Result<()> {
            self.infra
                .file_write_service()
                .write(path, Bytes::from(content.to_string()))
                .await
        }

        // Read a config from a specific location
        async fn read_config(&self, path: &Path) -> anyhow::Result<McpConfig> {
            let content = self.infra.file_read_service().read_utf8(path).await?;
            Ok(serde_json::from_str(&content)?)
        }
    }

    #[tokio::test]
    async fn test_write_to_user_scope() {
        let fixture = TestFixture::new();
        let manager = fixture.manager();
        let config = create_test_config("user-server");

        let write_result = manager.write(&config, &Scope::User).await;

        assert!(write_result.is_ok());

        let read_config = fixture
            .read_config(
                &fixture
                    .infra
                    .environment_service()
                    .get_environment()
                    .mcp_user_config(),
            )
            .await
            .unwrap();
        assert_eq!(read_config, config);
    }

    #[tokio::test]
    async fn test_write_to_local_scope() {
        let fixture = TestFixture::new();
        let manager = fixture.manager();
        let config = create_test_config("new-local-server");

        let write_result = manager.write(&config, &Scope::Local).await;

        assert!(write_result.is_ok());

        let read_config = fixture
            .read_config(
                &fixture
                    .infra
                    .environment_service()
                    .get_environment()
                    .mcp_local_config(),
            )
            .await
            .unwrap();
        assert_eq!(read_config, config);
    }

    #[tokio::test]
    async fn test_read_with_no_configs() {
        // Fixture
        let fixture = TestFixture::new();
        let manager = fixture.manager();

        // Actual
        let actual = manager.read().await.unwrap();

        // Expected
        let expected = McpConfig::default();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_with_user_config_only() {
        let fixture = TestFixture::new();
        let manager = fixture.manager();

        let server_name = "user-server";
        let user_config = create_test_config(server_name);

        manager.write(&user_config, &Scope::User).await.unwrap();

        let actual = manager.read().await.unwrap();

        assert_eq!(actual, user_config);
    }

    #[tokio::test]
    async fn test_read_with_local_config_only() {
        let fixture = TestFixture::new();
        let manager = fixture.manager();

        let server_name = "local-server";
        let local_config = create_test_config(server_name);
        manager.write(&local_config, &Scope::Local).await.unwrap();

        let actual = manager.read().await.unwrap();

        assert_eq!(actual, local_config);
    }

    #[tokio::test]
    async fn test_read_with_priority() {
        let fixture = TestFixture::new();
        let manager = fixture.manager();

        let common_server = "common-server";
        let user_server = "user-server";
        let local_server = "local-server";

        let mut user_config = create_test_config(common_server);
        let user_config_ext = create_test_config(user_server);
        // Manually call extend to cover tests of `merge`.
        user_config.mcp_servers.extend(user_config_ext.mcp_servers);

        // Local config with two servers (one overriding user config)
        let mut local_config = create_test_config(common_server);
        *local_config.mcp_servers.get_mut(common_server).unwrap() =
            McpServerConfig::Sse(McpSseServer { url: "override".to_string() });
        let local_config_ext = create_test_config(local_server);
        local_config
            .mcp_servers
            .extend(local_config_ext.mcp_servers);

        manager.write(&user_config, &Scope::User).await.unwrap();
        manager.write(&local_config, &Scope::Local).await.unwrap();

        let actual = manager.read().await.unwrap();

        // Expected: merged config with local taking priority
        // Check for overridden server configuration
        let common_server_config = actual.mcp_servers.get(common_server).unwrap();

        match common_server_config {
            McpServerConfig::Sse(sse) => {
                assert_eq!(sse.url, "override");
            }
            _ => panic!("Expected stdio server"),
        }

        // Check both unique servers exist
        assert!(actual.mcp_servers.contains_key(user_server));
        assert!(actual.mcp_servers.contains_key(local_server));
    }

    #[tokio::test]
    async fn test_read_with_invalid_user_config() {
        // Fixture
        let fixture = TestFixture::new();
        let manager = fixture.manager();

        // Write invalid JSON to user config
        fixture
            .write_raw(
                &fixture
                    .infra
                    .environment_service()
                    .get_environment()
                    .mcp_user_config(),
                "invalid json",
            )
            .await
            .unwrap();

        let local_config = create_test_config("local-server");
        manager.write(&local_config, &Scope::Local).await.unwrap();

        let actual = manager.read().await;

        assert!(actual.is_err());
        let error = actual.unwrap_err().to_string();
        assert!(error.contains("Invalid config"));
    }
}
