use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use forge_app::domain::{McpConfig, McpServerConfig, Scope, ServerName};
use forge_app::{
    EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra, KVStore, McpConfigManager,
    McpServerInfra,
};
use forge_domain::PluginRepository;
use merge::Merge;

/// Environment variable names injected into plugin-contributed stdio MCP
/// servers so the subprocess can locate its plugin root and the current
/// Forge workspace. HTTP-transport servers don't receive these because
/// they don't spawn a subprocess.
const FORGE_PLUGIN_ROOT_ENV: &str = "FORGE_PLUGIN_ROOT";
const FORGE_PROJECT_DIR_ENV: &str = "FORGE_PROJECT_DIR";

/// Claude Code compatibility aliases — injected alongside the `FORGE_*`
/// counterparts so marketplace plugins that reference `$CLAUDE_*` variables
/// work under Forge without modification.
const CLAUDE_PLUGIN_ROOT_ENV_ALIAS: &str = "CLAUDE_PLUGIN_ROOT";
const CLAUDE_PROJECT_DIR_ENV_ALIAS: &str = "CLAUDE_PROJECT_DIR";

pub struct ForgeMcpManager<I> {
    infra: Arc<I>,
    /// Optional plugin repository used to discover plugin-contributed MCP
    /// servers. Wrapped in `Option` so tests and legacy wiring paths that
    /// don't care about plugins can omit it without breaking construction.
    plugin_repository: Option<Arc<dyn PluginRepository>>,
}

impl<I> ForgeMcpManager<I>
where
    I: McpServerInfra + FileReaderInfra + FileInfoInfra + EnvironmentInfra + KVStore,
{
    /// Creates a manager without any plugin-contributed MCP servers.
    /// Prefer [`ForgeMcpManager::with_plugin_repository`] in production
    /// wiring so `/plugin` and plugin-shipped MCP configs take effect.
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra, plugin_repository: None }
    }

    /// Creates a manager that will merge plugin-contributed MCP servers
    /// (under the `"{plugin}:{server}"` namespace) into the output of
    /// [`McpConfigManager::read_mcp_config`] whenever the `None` scope is
    /// requested.
    pub fn with_plugin_repository(
        infra: Arc<I>,
        plugin_repository: Arc<dyn PluginRepository>,
    ) -> Self {
        Self { infra, plugin_repository: Some(plugin_repository) }
    }

    async fn read_config(&self, path: &Path) -> anyhow::Result<McpConfig> {
        let config = self.infra.read_utf8(path).await?;
        Ok(serde_json::from_str(&config)?)
    }

    async fn config_path(&self, scope: &Scope) -> anyhow::Result<PathBuf> {
        let env = self.infra.get_environment();
        match scope {
            Scope::User => Ok(env.mcp_user_config()),
            Scope::Local => Ok(env.mcp_local_config()),
        }
    }
}

/// Plugin-discovery impl block. Only requires [`EnvironmentInfra`] (to
/// read `cwd` for the `FORGE_PROJECT_DIR` env var) so unit tests can
/// instantiate a stub infra without having to implement the full
/// file/KV/MCP surface that [`McpConfigManager`] needs.
impl<I> ForgeMcpManager<I>
where
    I: EnvironmentInfra,
{
    /// Discovers MCP servers contributed by enabled plugins and returns
    /// them as an [`McpConfig`] whose server names are namespaced with
    /// the plugin name (e.g. `"acme:db"`) to avoid collisions with
    /// user/project/local scopes — and with each other.
    ///
    /// For stdio-transport plugin servers, `FORGE_PLUGIN_ROOT` and
    /// `FORGE_PROJECT_DIR` are injected into the subprocess environment
    /// so the server can locate its own resources and the current
    /// workspace. HTTP-transport servers are forwarded as-is because
    /// they don't spawn a subprocess.
    ///
    /// Returns an empty config when no plugin repository is configured
    /// or when the repository yields no enabled plugins with MCP
    /// servers.
    async fn load_plugin_mcp_servers(&self) -> anyhow::Result<McpConfig> {
        let Some(plugin_repo) = self.plugin_repository.as_ref() else {
            return Ok(McpConfig::default());
        };

        let plugins = plugin_repo
            .load_plugins()
            .await
            .context("Failed to load plugins while building MCP config")?;

        let env = self.infra.get_environment();
        let project_dir = env.cwd.display().to_string();

        let mut servers: BTreeMap<ServerName, McpServerConfig> = BTreeMap::new();
        for plugin in plugins.into_iter().filter(|p| p.enabled) {
            let Some(plugin_servers) = plugin.mcp_servers.as_ref() else {
                continue;
            };
            let plugin_root = plugin.path.display().to_string();

            for (server_name, server_cfg) in plugin_servers {
                let namespaced: ServerName = format!("{}:{}", plugin.name, server_name).into();

                // Inject plugin-awareness env vars into stdio subprocesses.
                // HTTP servers fall through unchanged.
                let cfg = match server_cfg.clone() {
                    McpServerConfig::Stdio(mut stdio) => {
                        stdio
                            .env
                            .entry(FORGE_PLUGIN_ROOT_ENV.to_string())
                            .or_insert_with(|| plugin_root.clone());
                        stdio
                            .env
                            .entry(CLAUDE_PLUGIN_ROOT_ENV_ALIAS.to_string())
                            .or_insert_with(|| plugin_root.clone());
                        stdio
                            .env
                            .entry(FORGE_PROJECT_DIR_ENV.to_string())
                            .or_insert_with(|| project_dir.clone());
                        stdio
                            .env
                            .entry(CLAUDE_PROJECT_DIR_ENV_ALIAS.to_string())
                            .or_insert_with(|| project_dir.clone());
                        McpServerConfig::Stdio(stdio)
                    }
                    other => other,
                };

                servers.insert(namespaced, cfg);
            }
        }

        Ok(McpConfig::from(servers))
    }
}

#[async_trait::async_trait]
impl<I> McpConfigManager for ForgeMcpManager<I>
where
    I: McpServerInfra
        + FileReaderInfra
        + FileInfoInfra
        + EnvironmentInfra
        + FileWriterInfra
        + KVStore,
{
    async fn read_mcp_config(&self, scope: Option<&Scope>) -> anyhow::Result<McpConfig> {
        match scope {
            Some(scope) => {
                // Read only from the specified scope
                let config_path = self.config_path(scope).await?;
                if self.infra.is_file(&config_path).await.unwrap_or(false) {
                    self.read_config(&config_path).await
                } else {
                    Ok(McpConfig::default())
                }
            }
            None => {
                // Read and merge all configurations (original behavior)
                let env = self.infra.get_environment();
                let paths = vec![
                    // Configs at lower levels take precedence, so we read them in reverse order.
                    env.mcp_user_config().as_path().to_path_buf(),
                    env.mcp_local_config().as_path().to_path_buf(),
                ];
                let mut config = McpConfig::default();
                for path in paths {
                    if self.infra.is_file(&path).await.unwrap_or_default() {
                        let new_config = self.read_config(&path).await.context(format!(
                            "An error occurred while reading config at: {}",
                            path.display()
                        ))?;
                        config.merge(new_config);
                    }
                }

                // Plugin-contributed MCP servers. Merged last so plugin
                // servers can appear alongside the built-in scopes, but
                // the `"{plugin}:{server}"` namespace guarantees no
                // collision with project/user/local entries.
                let plugin_config = self.load_plugin_mcp_servers().await?;
                config.merge(plugin_config);
                Ok(config)
            }
        }
    }

    async fn write_mcp_config(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()> {
        // Write config
        self.infra
            .write(
                self.config_path(scope).await?.as_path(),
                Bytes::from(serde_json::to_string_pretty(config)?),
            )
            .await?;

        // Clear the unified cache to force refresh on next use
        // Since we now use a merged hash, clearing any scope invalidates the cache
        self.infra.cache_clear().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_app::domain::{McpServerConfig, McpStdioServer};
    use forge_domain::{
        LoadedPlugin, PluginLoadResult, PluginManifest, PluginRepository, PluginSource,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    /// Test-only [`PluginRepository`] backed by a fixed plugin list. Mirrors
    /// the pattern used by `forge_services::hook_runtime::config_loader`
    /// and `forge_repo::skill` tests.
    #[derive(Default)]
    struct MockPluginRepository {
        plugins: Mutex<Vec<LoadedPlugin>>,
    }

    impl MockPluginRepository {
        fn with(plugins: Vec<LoadedPlugin>) -> Self {
            Self { plugins: Mutex::new(plugins) }
        }
    }

    #[async_trait]
    impl PluginRepository for MockPluginRepository {
        async fn load_plugins(&self) -> anyhow::Result<Vec<LoadedPlugin>> {
            Ok(self.plugins.lock().unwrap().clone())
        }

        async fn load_plugins_with_errors(&self) -> anyhow::Result<PluginLoadResult> {
            Ok(PluginLoadResult::new(
                self.plugins.lock().unwrap().clone(),
                Vec::new(),
            ))
        }
    }

    /// Stub infra: the plugin-discovery helper only touches
    /// `get_environment().cwd`. The rest of the [`McpConfigManager`] trait
    /// bounds are irrelevant to these unit tests because we call
    /// `load_plugin_mcp_servers` directly.
    struct StubInfra {
        cwd: PathBuf,
    }

    impl StubInfra {
        fn new(cwd: PathBuf) -> Self {
            Self { cwd }
        }
    }

    impl forge_app::EnvironmentInfra for StubInfra {
        type Config = forge_config::ForgeConfig;

        fn get_environment(&self) -> forge_domain::Environment {
            forge_domain::Environment {
                os: "linux".to_string(),
                cwd: self.cwd.clone(),
                home: Some(self.cwd.clone()),
                shell: "/bin/bash".to_string(),
                base_path: self.cwd.clone(),
            }
        }

        fn get_config(&self) -> anyhow::Result<forge_config::ForgeConfig> {
            Ok(forge_config::ForgeConfig::default())
        }

        async fn update_environment(
            &self,
            _ops: Vec<forge_domain::ConfigOperation>,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> BTreeMap<String, String> {
            BTreeMap::new()
        }
    }

    fn plugin(
        name: &str,
        enabled: bool,
        servers: Option<BTreeMap<String, McpServerConfig>>,
    ) -> LoadedPlugin {
        LoadedPlugin {
            name: name.to_string(),
            manifest: PluginManifest { name: Some(name.to_string()), ..Default::default() },
            path: PathBuf::from(format!("/tmp/plugins/{name}")),
            source: PluginSource::Global,
            enabled,
            is_builtin: false,
            commands_paths: Vec::new(),
            agents_paths: Vec::new(),
            skills_paths: Vec::new(),
            mcp_servers: servers,
        }
    }

    fn stdio_server(command: &str) -> McpServerConfig {
        McpServerConfig::Stdio(McpStdioServer {
            command: command.to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            timeout: None,
            disable: false,
        })
    }

    fn manager_with(plugins: Vec<LoadedPlugin>) -> ForgeMcpManager<StubInfra> {
        let infra = Arc::new(StubInfra::new(PathBuf::from("/workspace/test")));
        let repo: Arc<dyn PluginRepository> = Arc::new(MockPluginRepository::with(plugins));
        ForgeMcpManager { infra, plugin_repository: Some(repo) }
    }

    #[tokio::test]
    async fn test_load_plugin_mcp_servers_empty_when_no_plugin_repo() {
        let fixture = ForgeMcpManager {
            infra: Arc::new(StubInfra::new(PathBuf::from("/workspace/test"))),
            plugin_repository: None,
        };

        let actual = fixture.load_plugin_mcp_servers().await.unwrap();

        assert_eq!(actual.mcp_servers.len(), 0);
    }

    #[tokio::test]
    async fn test_load_plugin_mcp_servers_empty_when_no_plugins() {
        let fixture = manager_with(Vec::new());

        let actual = fixture.load_plugin_mcp_servers().await.unwrap();

        assert_eq!(actual.mcp_servers.len(), 0);
    }

    #[tokio::test]
    async fn test_load_plugin_mcp_servers_namespaces_correctly() {
        let mut servers = BTreeMap::new();
        servers.insert("db".to_string(), stdio_server("acme-db"));
        let fixture = manager_with(vec![plugin("acme", true, Some(servers))]);

        let actual = fixture.load_plugin_mcp_servers().await.unwrap();

        let key: ServerName = "acme:db".to_string().into();
        assert!(
            actual.mcp_servers.contains_key(&key),
            "expected namespaced key 'acme:db', got: {:?}",
            actual.mcp_servers.keys().collect::<Vec<_>>()
        );
        assert_eq!(actual.mcp_servers.len(), 1);
    }

    #[tokio::test]
    async fn test_load_plugin_mcp_servers_skips_disabled_plugins() {
        let mut servers_on = BTreeMap::new();
        servers_on.insert("svc".to_string(), stdio_server("alive"));
        let mut servers_off = BTreeMap::new();
        servers_off.insert("svc".to_string(), stdio_server("dead"));
        let fixture = manager_with(vec![
            plugin("enabled-plug", true, Some(servers_on)),
            plugin("disabled-plug", false, Some(servers_off)),
        ]);

        let actual = fixture.load_plugin_mcp_servers().await.unwrap();

        let enabled_key: ServerName = "enabled-plug:svc".to_string().into();
        let disabled_key: ServerName = "disabled-plug:svc".to_string().into();
        assert!(actual.mcp_servers.contains_key(&enabled_key));
        assert!(!actual.mcp_servers.contains_key(&disabled_key));
        assert_eq!(actual.mcp_servers.len(), 1);
    }

    #[tokio::test]
    async fn test_load_plugin_mcp_servers_multiple_plugins_no_collision() {
        let mut servers_a = BTreeMap::new();
        servers_a.insert("shared".to_string(), stdio_server("from-a"));
        let mut servers_b = BTreeMap::new();
        servers_b.insert("shared".to_string(), stdio_server("from-b"));
        let fixture = manager_with(vec![
            plugin("plugin-a", true, Some(servers_a)),
            plugin("plugin-b", true, Some(servers_b)),
        ]);

        let actual = fixture.load_plugin_mcp_servers().await.unwrap();

        assert_eq!(actual.mcp_servers.len(), 2);
        let key_a: ServerName = "plugin-a:shared".to_string().into();
        let key_b: ServerName = "plugin-b:shared".to_string().into();
        assert!(actual.mcp_servers.contains_key(&key_a));
        assert!(actual.mcp_servers.contains_key(&key_b));

        // Confirm the two servers are distinct (their inner commands
        // should differ).
        let cmd_a = match actual.mcp_servers.get(&key_a).unwrap() {
            McpServerConfig::Stdio(s) => s.command.clone(),
            _ => panic!("expected stdio"),
        };
        let cmd_b = match actual.mcp_servers.get(&key_b).unwrap() {
            McpServerConfig::Stdio(s) => s.command.clone(),
            _ => panic!("expected stdio"),
        };
        assert_eq!(cmd_a, "from-a");
        assert_eq!(cmd_b, "from-b");
    }

    #[tokio::test]
    async fn test_load_plugin_mcp_servers_injects_forge_env_vars_for_stdio() {
        let mut servers = BTreeMap::new();
        servers.insert("svc".to_string(), stdio_server("bin"));
        let fixture = manager_with(vec![plugin("acme", true, Some(servers))]);

        let actual = fixture.load_plugin_mcp_servers().await.unwrap();

        let key: ServerName = "acme:svc".to_string().into();
        let stdio = match actual.mcp_servers.get(&key).unwrap() {
            McpServerConfig::Stdio(s) => s,
            _ => panic!("expected stdio"),
        };
        assert_eq!(
            stdio.env.get(FORGE_PLUGIN_ROOT_ENV).map(String::as_str),
            Some("/tmp/plugins/acme")
        );
        assert_eq!(
            stdio.env.get(CLAUDE_PLUGIN_ROOT_ENV_ALIAS).map(String::as_str),
            Some("/tmp/plugins/acme")
        );
        assert_eq!(
            stdio.env.get(FORGE_PROJECT_DIR_ENV).map(String::as_str),
            Some("/workspace/test")
        );
        assert_eq!(
            stdio.env.get(CLAUDE_PROJECT_DIR_ENV_ALIAS).map(String::as_str),
            Some("/workspace/test")
        );
    }

    #[tokio::test]
    async fn test_load_plugin_mcp_servers_preserves_existing_env_vars() {
        // If the plugin author already set FORGE_PLUGIN_ROOT (e.g. for
        // a test harness), we must not clobber it.
        let mut env = BTreeMap::new();
        env.insert(FORGE_PLUGIN_ROOT_ENV.to_string(), "/custom".to_string());
        env.insert("USER_VAR".to_string(), "x".to_string());
        let stdio = McpServerConfig::Stdio(McpStdioServer {
            command: "bin".to_string(),
            args: Vec::new(),
            env,
            timeout: None,
            disable: false,
        });
        let mut servers = BTreeMap::new();
        servers.insert("svc".to_string(), stdio);
        let fixture = manager_with(vec![plugin("acme", true, Some(servers))]);

        let actual = fixture.load_plugin_mcp_servers().await.unwrap();

        let key: ServerName = "acme:svc".to_string().into();
        let stdio = match actual.mcp_servers.get(&key).unwrap() {
            McpServerConfig::Stdio(s) => s,
            _ => panic!("expected stdio"),
        };
        assert_eq!(
            stdio.env.get(FORGE_PLUGIN_ROOT_ENV).map(String::as_str),
            Some("/custom"),
            "existing FORGE_PLUGIN_ROOT should be preserved"
        );
        assert_eq!(stdio.env.get("USER_VAR").map(String::as_str), Some("x"));
        // But FORGE_PROJECT_DIR should still be injected because it
        // wasn't present.
        assert_eq!(
            stdio.env.get(FORGE_PROJECT_DIR_ENV).map(String::as_str),
            Some("/workspace/test")
        );
        // CLAUDE_* aliases should also be injected.
        assert_eq!(
            stdio.env.get(CLAUDE_PLUGIN_ROOT_ENV_ALIAS).map(String::as_str),
            Some("/tmp/plugins/acme"),
            "CLAUDE_PLUGIN_ROOT should be injected from plugin path"
        );
        assert_eq!(
            stdio.env.get(CLAUDE_PROJECT_DIR_ENV_ALIAS).map(String::as_str),
            Some("/workspace/test")
        );
    }
}
