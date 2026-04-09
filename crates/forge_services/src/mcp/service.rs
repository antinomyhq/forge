use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::{
    McpConfig, McpServers, ServerName, ToolCallFull, ToolDefinition, ToolName, ToolOutput,
};
use forge_app::{
    EnvironmentInfra, KVStore, McpClientInfra, McpConfigManager, McpServerInfra, McpService,
};
use tokio::sync::{Mutex, RwLock};

use crate::mcp::lazy_client::LazyMcpClient;
use crate::mcp::tool::McpExecutor;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub struct ForgeMcpService<M, I: McpServerInfra, C> {
    /// Live, connected tool executors (populated on first actual tool call).
    tools: Arc<RwLock<HashMap<ToolName, ToolHolder<McpExecutor<C>>>>>,
    /// Servers registered from config – connection is NOT yet established.
    pending_servers: Arc<RwLock<HashMap<ServerName, LazyMcpClient<I>>>>,
    failed_servers: Arc<RwLock<HashMap<ServerName, String>>>,
    /// Tool stubs built from statically-declared tools in config (no live connection).
    declared_tools: Arc<RwLock<HashMap<ToolName, ServerName>>>,
    previous_config_hash: Arc<Mutex<u64>>,
    manager: Arc<M>,
    infra: Arc<I>,
}

/// Manual `Clone` impl so that we don't require `M: Clone` or `I: Clone` —
/// all fields are `Arc`-wrapped so the clone just bumps reference counts.
impl<M, I: McpServerInfra, C: Clone> Clone for ForgeMcpService<M, I, C> {
    fn clone(&self) -> Self {
        Self {
            tools: self.tools.clone(),
            pending_servers: self.pending_servers.clone(),
            failed_servers: self.failed_servers.clone(),
            declared_tools: self.declared_tools.clone(),
            previous_config_hash: self.previous_config_hash.clone(),
            manager: self.manager.clone(),
            infra: self.infra.clone(),
        }
    }
}

#[derive(Clone)]
struct ToolHolder<T> {
    definition: ToolDefinition,
    executable: T,
    server_name: String,
}

// ---------------------------------------------------------------------------
// Core implementation
// ---------------------------------------------------------------------------

impl<M, I, C> ForgeMcpService<M, I, C>
where
    M: McpConfigManager,
    I: McpServerInfra + KVStore + EnvironmentInfra,
    C: McpClientInfra + Clone,
    C: From<<I as McpServerInfra>::Client>,
{
    pub fn new(manager: Arc<M>, infra: Arc<I>) -> Self {
        Self {
            tools: Default::default(),
            pending_servers: Default::default(),
            failed_servers: Default::default(),
            declared_tools: Default::default(),
            previous_config_hash: Arc::new(Mutex::new(Default::default())),
            manager,
            infra,
        }
    }

    async fn is_config_modified(&self, config: &McpConfig) -> bool {
        *self.previous_config_hash.lock().await != config.cache_key()
    }

    // -----------------------------------------------------------------------
    // Discovery — zero network I/O
    // -----------------------------------------------------------------------

    /// Register servers from config without establishing any connection.
    ///
    /// For servers that declare their tools statically in the config we
    /// immediately build lightweight `ToolDefinition` stubs so the LLM can see
    /// them in the system prompt. For servers with no static declarations, the
    /// tools remain invisible until one of their tools is actually called and
    /// the live connection is established.
    async fn register_servers(&self, mcp: McpConfig) {
        let new_hash = mcp.cache_key();
        *self.previous_config_hash.lock().await = new_hash;

        // Any config change — even adding an unrelated server — evicts all live
        // connections and pending registrations. This matches the previous eager
        // connect-everything behaviour and avoids stale tool lists for servers
        // whose config did change. A future optimisation could diff the configs
        // and preserve live connections for unchanged servers.
        self.tools.write().await.clear();
        self.pending_servers.write().await.clear();
        self.declared_tools.write().await.clear();
        self.failed_servers.write().await.clear();

        let env_vars = self.infra.get_env_vars();

        let mut pending = self.pending_servers.write().await;
        let mut declared = self.declared_tools.write().await;

        for (server_name, config) in mcp.mcp_servers.into_iter().filter(|v| !v.1.is_disabled()) {
            // Build a lazy client – no network call happens here
            let lazy = LazyMcpClient::new(config.clone(), env_vars.clone(), self.infra.clone());
            pending.insert(server_name.clone(), lazy);

            // Populate declared-tool stubs if the config specifies tool names
            if let Some(tool_names) = config.declared_tools() {
                for raw_name in tool_names {
                    let generated = ToolName::new(format!(
                        "mcp_{server_name}_tool_{}",
                        ToolName::sanitized(raw_name)
                    ));
                    declared.insert(generated, server_name.clone());
                }
            }
        }
    }

    /// Ensure servers are registered from the current config.
    /// Called at the start of `list()` and `call()`.
    async fn ensure_registered(&self) -> anyhow::Result<()> {
        let mcp = self.manager.read_mcp_config(None).await?;
        if self.is_config_modified(&mcp).await {
            self.register_servers(mcp).await;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Connection — happens only when a tool is actually invoked
    // -----------------------------------------------------------------------

    /// Determine which server owns `tool_name` and connect to it.
    async fn connect_for_tool(&self, tool_name: &ToolName) -> anyhow::Result<()> {
        // Try to find the owning server via declared-tool stubs first (no-alloc path)
        let server_name = {
            let declared = self.declared_tools.read().await;
            declared.get(tool_name).cloned()
        };

        match server_name {
            Some(name) => self.connect_server(&name).await,
            None => {
                // Tool was not declared statically; discover by connecting all pending servers.
                self.connect_all_pending().await
            }
        }
    }

    /// Connect a specific server by name and insert its tools.
    ///
    /// **Concurrency:** the `pending_servers.remove()` call below is the true
    /// mutual-exclusion point — exactly one concurrent caller receives the
    /// `LazyMcpClient`; all others get `None` and return `Ok(())`.
    ///
    /// The two read-lock checks that precede it (fast-path on `tools` and on
    /// `failed_servers`) are *optimistic* guards.  They avoid the write-lock in
    /// the common case but are not atomic with respect to each other: a server
    /// could be marked failed between the two reads.  The consequence is benign
    /// — the `pending_servers.remove()` will return `None` and the caller
    /// returns `Ok(())`, seeing the failure on its next attempt.
    async fn connect_server(&self, server_name: &ServerName) -> anyhow::Result<()> {
        // Fast path — already connected (no write-lock needed).
        {
            let tools = self.tools.read().await;
            if tools
                .values()
                .any(|h| h.server_name == server_name.as_str())
            {
                return Ok(());
            }
        }

        // Already failed?
        {
            let failed = self.failed_servers.read().await;
            if let Some(err) = failed.get(server_name) {
                return Err(anyhow::anyhow!(
                    "MCP server '{server_name}' previously failed to connect: {err}"
                ));
            }
        }

        // Grab the lazy client and remove it from pending in one step so
        // concurrent callers that also passed the fast-path check cannot both
        // proceed to connect the same server (TOCTOU fix).
        let lazy = {
            let mut pending = self.pending_servers.write().await;
            match pending.remove(server_name) {
                Some(lazy) => lazy,
                // Another concurrent caller already removed and connected it.
                None => return Ok(()),
            }
        };

        // Trigger the actual connection + list tools
        match self.insert_lazy_client(server_name, lazy).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let msg = format!("{e:?}");
                self.failed_servers
                    .write()
                    .await
                    .insert(server_name.clone(), msg.clone());
                Err(anyhow::anyhow!(
                    "Failed to connect to MCP server '{server_name}': {msg}"
                ))
            }
        }
    }

    /// Connect all pending servers (used when the tool owner is unknown).
    ///
    /// Connections are driven concurrently via `join_all`.  All fields of
    /// `ForgeMcpService` are `Arc`-wrapped so cloning is cheap.
    async fn connect_all_pending(&self) -> anyhow::Result<()> {
        let pending_names: Vec<ServerName> = {
            let pending = self.pending_servers.read().await;
            pending.keys().cloned().collect()
        };

        // Clone `self` once per server so each async block owns its own handle
        // to the shared state.  This avoids lifetime issues with `&self` across
        // yield points inside `join_all`.
        let futures: Vec<_> = pending_names
            .into_iter()
            .map(|name| {
                let svc = self.clone();
                async move { svc.connect_server(&name).await }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        let mut last_err: Option<anyhow::Error> = None;
        for r in results {
            if let Err(e) = r {
                tracing::warn!(error = ?e, "MCP server failed to connect during bulk discovery");
                last_err = Some(e);
            }
        }

        // Return last error only if nothing was connected at all
        if let Some(e) = last_err {
            let tools = self.tools.read().await;
            if tools.is_empty() {
                return Err(e);
            }
        }
        Ok(())
    }

    /// Call `list()` on the lazy client (triggers real connection) and insert
    /// the resulting tools into the live tool map.
    async fn insert_lazy_client(
        &self,
        server_name: &ServerName,
        lazy: LazyMcpClient<I>,
    ) -> anyhow::Result<()> {
        // `lazy.list()` triggers the real connection on first call
        let live_tools = lazy.list().await?;

        // Re-use the already-initialised inner client for execution
        let inner = lazy.into_inner().await?;
        let client: Arc<C> = Arc::new(C::from(inner));
        let mut tool_map = self.tools.write().await;

        for mut tool in live_tools.into_iter() {
            let actual_name = tool.name.clone();
            let executor = McpExecutor::new(actual_name, client.clone())?;

            let generated_name = ToolName::new(format!(
                "mcp_{server_name}_tool_{}",
                tool.name.into_sanitized()
            ));

            tool.name = generated_name.clone();

            tool_map.insert(
                generated_name,
                ToolHolder {
                    definition: tool,
                    executable: executor,
                    server_name: server_name.to_string(),
                },
            );
        }
        // Drop the write lock on tools before taking one on declared_tools to
        // avoid holding two write locks simultaneously.
        drop(tool_map);

        // Prune stale declared stubs for this server now that we have the live list.
        // This prevents contains_tool_in_memory from returning true for tools that
        // were declared in config but absent from the server's actual live tool list.
        self.declared_tools
            .write()
            .await
            .retain(|_, owner| owner != server_name);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // list() — returns tool stubs without blocking on connections
    // -----------------------------------------------------------------------

    async fn list(&self) -> anyhow::Result<McpServers> {
        self.ensure_registered().await?;

        let tools = self.tools.read().await;
        let declared = self.declared_tools.read().await;
        let failures = self.failed_servers.read().await.clone();

        let mut grouped: HashMap<ServerName, Vec<ToolDefinition>> = HashMap::new();

        // Include already-connected live tools (full schemas)
        for holder in tools.values() {
            grouped
                .entry(ServerName::from(holder.server_name.clone()))
                .or_default()
                .push(holder.definition.clone());
        }

        // Include declared (not-yet-connected) tool stubs
        for (tool_name, server_name) in declared.iter() {
            // Skip if already represented via a live connection
            if tools.contains_key(tool_name) {
                continue;
            }
            let stub = ToolDefinition::new(tool_name.as_str())
                .description("(tool schema not yet loaded — schema available after first use)");
            grouped.entry(server_name.clone()).or_default().push(stub);
        }

        Ok(McpServers::new(grouped, failures))
    }

    // -----------------------------------------------------------------------
    // call() — triggers real connection lazily
    // -----------------------------------------------------------------------

    async fn call(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        self.ensure_registered().await?;

        // Fast path: tool already live
        {
            let tools = self.tools.read().await;
            if let Some(holder) = tools.get(&call.name) {
                return holder.executable.call_tool(call.arguments.parse()?).await;
            }
        }

        // Slow path: connect the owning server on first use
        self.connect_for_tool(&call.name).await?;

        let tools = self.tools.read().await;
        let holder = tools.get(&call.name).context(format!(
            "Tool '{}' not found after connecting MCP server",
            call.name
        ))?;
        holder.executable.call_tool(call.arguments.parse()?).await
    }

    // -----------------------------------------------------------------------
    // contains_tool — in-memory only, zero network I/O
    // -----------------------------------------------------------------------

    async fn contains_tool_in_memory(&self, tool_name: &ToolName) -> bool {
        if self.tools.read().await.contains_key(tool_name) {
            return true;
        }
        self.declared_tools.read().await.contains_key(tool_name)
    }

    // -----------------------------------------------------------------------
    // Refresh
    // -----------------------------------------------------------------------

    async fn refresh_cache(&self) -> anyhow::Result<()> {
        self.infra.cache_clear().await?;
        // Reset config hash so the next access re-registers from disk
        *self.previous_config_hash.lock().await = 0;
        let _ = self.get_mcp_servers().await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// McpService trait implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl<M: McpConfigManager, I: McpServerInfra + KVStore + EnvironmentInfra, C> McpService
    for ForgeMcpService<M, I, C>
where
    C: McpClientInfra + Clone,
    C: From<<I as McpServerInfra>::Client>,
{
    async fn get_mcp_servers(&self) -> anyhow::Result<McpServers> {
        let mcp_config = self.manager.read_mcp_config(None).await?;
        let config_hash = mcp_config.cache_key();

        // The KV cache stores *stub* tool listings (declared-tool names only,
        // no full schemas) so that cold-start latency is minimised.  Once any
        // server has established a live connection its full schemas live in the
        // in-memory `tools` map which is always authoritative and must not be
        // replaced by a stale KV entry.
        //
        // Decision matrix:
        //   no live connections, KV hit  → serve KV cache (stubs, fast path)
        //   no live connections, KV miss → build from declared config, write KV
        //   any live connections         → serve from memory (includes schemas)
        //                                  do NOT write back to KV (avoid
        //                                  persisting schemas that may become
        //                                  stale across runs)
        //
        // Note: `had_live_before` and `has_live_now` are point-in-time snapshots.
        // A concurrent caller could establish connections between either read and
        // the subsequent operations.  This is an intentional eventual-consistency
        // tradeoff: the window is sub-millisecond and the worst case is a missed
        // cache write (not a correctness failure).
        let had_live_before = !self.tools.read().await.is_empty();
        if !had_live_before {
            if let Some(cache) = self.infra.cache_get::<_, McpServers>(&config_hash).await? {
                return Ok(cache);
            }
        }

        let servers = self.list().await?;

        // Fresh read after list() so we never persist live schemas to the KV cache.
        // list() may have triggered connections (e.g. via ensure_registered clearing
        // and re-registering), so take an up-to-date snapshot.
        let has_live_now = !self.tools.read().await.is_empty();
        if !has_live_now {
            self.infra.cache_set(&config_hash, &servers).await?;
        }

        Ok(servers)
    }

    async fn execute_mcp(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        self.call(call).await
    }

    async fn reload_mcp(&self) -> anyhow::Result<()> {
        self.refresh_cache().await
    }

    async fn contains_mcp_tool(&self, tool_name: &ToolName) -> anyhow::Result<bool> {
        self.ensure_registered().await?;
        Ok(self.contains_tool_in_memory(tool_name).await)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_app::domain::{
        McpConfig, McpServerConfig, McpStdioServer, ServerName, ToolCallFull, ToolDefinition,
        ToolName, ToolOutput,
    };
    use forge_app::{
        EnvironmentInfra, KVStore, McpClientInfra, McpConfigManager, McpServerInfra, McpService,
    };
    use forge_config::ForgeConfig;
    use forge_domain::{ConfigOperation, Environment, Scope};
    use serde::de::DeserializeOwned;
    use tokio::sync::Mutex;

    use super::ForgeMcpService;

    // -----------------------------------------------------------------------
    // Mock: McpClientInfra
    // -----------------------------------------------------------------------

    #[derive(Clone)]
    struct MockMcpClientInfra {
        tools: Vec<ToolDefinition>,
        connect_count: Arc<AtomicUsize>,
    }

    impl MockMcpClientInfra {
        fn new(tool_names: Vec<&str>, connect_count: Arc<AtomicUsize>) -> Self {
            let tools = tool_names
                .into_iter()
                .map(|n| ToolDefinition::new(n))
                .collect();
            Self { tools, connect_count }
        }
    }

    #[async_trait::async_trait]
    impl McpClientInfra for MockMcpClientInfra {
        async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
            self.connect_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.tools.clone())
        }

        async fn call(
            &self,
            _tool_name: &ToolName,
            _input: serde_json::Value,
        ) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text("mock response"))
        }
    }

    // -----------------------------------------------------------------------
    // Mock: McpServerInfra + KVStore + EnvironmentInfra
    // -----------------------------------------------------------------------

    struct MockMcpServerInfra {
        client: MockMcpClientInfra,
        kv: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    }

    impl MockMcpServerInfra {
        fn new(client: MockMcpClientInfra) -> Self {
            Self { client, kv: Default::default() }
        }
    }

    #[async_trait::async_trait]
    impl McpServerInfra for MockMcpServerInfra {
        type Client = MockMcpClientInfra;

        async fn connect(
            &self,
            _config: McpServerConfig,
            _env_vars: &BTreeMap<String, String>,
        ) -> anyhow::Result<Self::Client> {
            Ok(self.client.clone())
        }
    }

    #[async_trait::async_trait]
    impl KVStore for MockMcpServerInfra {
        async fn cache_get<K, V>(&self, key: &K) -> anyhow::Result<Option<V>>
        where
            K: std::hash::Hash + Sync,
            V: serde::Serialize + DeserializeOwned + Send,
        {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::Hasher;
            let mut h = DefaultHasher::new();
            std::hash::Hash::hash(key, &mut h);
            let k = h.finish().to_string();
            let map = self.kv.lock().await;
            match map.get(&k) {
                Some(bytes) => Ok(Some(serde_json::from_slice(bytes)?)),
                None => Ok(None),
            }
        }

        async fn cache_set<K, V>(&self, key: &K, value: &V) -> anyhow::Result<()>
        where
            K: std::hash::Hash + Sync,
            V: serde::Serialize + Sync,
        {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::Hasher;
            let mut h = DefaultHasher::new();
            std::hash::Hash::hash(key, &mut h);
            let k = h.finish().to_string();
            let bytes = serde_json::to_vec(value)?;
            self.kv.lock().await.insert(k, bytes);
            Ok(())
        }

        async fn cache_clear(&self) -> anyhow::Result<()> {
            self.kv.lock().await.clear();
            Ok(())
        }
    }

    impl EnvironmentInfra for MockMcpServerInfra {
        type Config = ForgeConfig;

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> BTreeMap<String, String> {
            BTreeMap::new()
        }

        fn get_environment(&self) -> Environment {
            Environment {
                os: "test".to_string(),
                pid: 0,
                cwd: std::path::PathBuf::from("/tmp"),
                home: None,
                shell: "sh".to_string(),
                base_path: std::path::PathBuf::from("/tmp"),
            }
        }

        fn get_config(&self) -> ForgeConfig {
            ForgeConfig::default()
        }

        fn update_environment(
            &self,
            _ops: Vec<ConfigOperation>,
        ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
            async { Ok(()) }
        }
    }

    // -----------------------------------------------------------------------
    // Mock: McpConfigManager
    // -----------------------------------------------------------------------

    struct MockMcpConfigManager {
        config: Arc<Mutex<McpConfig>>,
    }

    impl MockMcpConfigManager {
        fn new(config: McpConfig) -> Self {
            Self { config: Arc::new(Mutex::new(config)) }
        }
    }

    #[async_trait::async_trait]
    impl McpConfigManager for MockMcpConfigManager {
        async fn read_mcp_config(&self, _scope: Option<&Scope>) -> anyhow::Result<McpConfig> {
            Ok(self.config.lock().await.clone())
        }

        async fn write_mcp_config(
            &self,
            _config: &McpConfig,
            _scope: &Scope,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn server_config_with_tools(tool_names: Vec<&str>) -> McpServerConfig {
        McpServerConfig::Stdio(McpStdioServer {
            command: "echo".to_string(),
            args: vec![],
            env: BTreeMap::new(),
            timeout: None,
            disable: false,
            tools: tool_names.into_iter().map(|s| s.to_string()).collect(),
        })
    }

    fn server_config_no_tools() -> McpServerConfig {
        McpServerConfig::new_stdio("echo", vec![], None)
    }

    fn make_service(
        config: McpConfig,
        client: MockMcpClientInfra,
    ) -> ForgeMcpService<MockMcpConfigManager, MockMcpServerInfra, MockMcpClientInfra> {
        let manager = Arc::new(MockMcpConfigManager::new(config));
        let infra = Arc::new(MockMcpServerInfra::new(client));
        ForgeMcpService::new(manager, infra)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// Declared tool names appear in get_mcp_servers() without any connection.
    #[tokio::test]
    async fn test_declared_tools_visible_without_connection() {
        let connect_count = Arc::new(AtomicUsize::new(0));
        let client = MockMcpClientInfra::new(vec![], connect_count.clone());

        let mut config = McpConfig::default();
        config.mcp_servers.insert(
            ServerName::from("github".to_string()),
            server_config_with_tools(vec!["get_repo", "list_prs"]),
        );

        let svc = make_service(config, client);
        let servers = svc.get_mcp_servers().await.unwrap();

        let tool_names: Vec<String> = servers
            .get_servers()
            .values()
            .flat_map(|tools| tools.iter().map(|t| t.name.to_string()))
            .collect();

        assert!(
            tool_names.iter().any(|n| n.contains("get_repo")),
            "expected get_repo in {tool_names:?}"
        );
        assert!(
            tool_names.iter().any(|n| n.contains("list_prs")),
            "expected list_prs in {tool_names:?}"
        );
        // No real connection should have been triggered
        assert_eq!(connect_count.load(Ordering::SeqCst), 0);
    }

    /// Calling a tool with no static declaration triggers connect_all_pending.
    #[tokio::test]
    async fn test_undeclared_tool_triggers_connect_all_pending() {
        let connect_count = Arc::new(AtomicUsize::new(0));
        // live tool returned by the server
        let client = MockMcpClientInfra::new(vec!["list_repos"], connect_count.clone());

        let mut config = McpConfig::default();
        config.mcp_servers.insert(
            ServerName::from("github".to_string()),
            server_config_no_tools(), // no static declaration
        );

        let svc = make_service(config, client);

        // Tool name that the live server would return (generated name)
        let call = ToolCallFull::new(ToolName::new("mcp_github_tool_list_repos"));
        let result = svc.execute_mcp(call).await;

        // Either succeeds (tool found) or fails with "not found" — the important
        // thing is that connect() was invoked.
        let _ = result; // don't assert success, just that path ran
        assert!(
            connect_count.load(Ordering::SeqCst) >= 1,
            "expected at least one connect() call"
        );
    }

    /// A declared tool triggers only the owning server's connection.
    #[tokio::test]
    async fn test_declared_tool_only_connects_owning_server() {
        let github_count = Arc::new(AtomicUsize::new(0));
        let notion_count = Arc::new(AtomicUsize::new(0));

        // github client: returns list_prs
        let github_client = MockMcpClientInfra::new(vec!["list_prs"], github_count.clone());

        // notion client: panics on list() — should never be called
        struct PanickingClient;
        #[async_trait::async_trait]
        impl McpClientInfra for PanickingClient {
            async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
                panic!("notion should not be connected")
            }
            async fn call(&self, _: &ToolName, _: serde_json::Value) -> anyhow::Result<ToolOutput> {
                panic!("should not be called")
            }
        }
        impl Clone for PanickingClient {
            fn clone(&self) -> Self {
                PanickingClient
            }
        }

        // Build a two-server config: github (declared) + notion (declared)
        // We set both as declared so connect_for_tool can route by name.
        let mut cfg = McpConfig::default();
        cfg.mcp_servers.insert(
            ServerName::from("github".to_string()),
            server_config_with_tools(vec!["list_prs"]),
        );
        cfg.mcp_servers.insert(
            ServerName::from("notion".to_string()),
            server_config_with_tools(vec!["search_pages"]),
        );

        // Use github client only — notion will never be reached
        let _ = notion_count; // suppress unused warning
        let svc = make_service(cfg, github_client);

        let call = ToolCallFull::new(ToolName::new("mcp_github_tool_list_prs"));
        let result = svc.execute_mcp(call).await;
        // Should succeed (github connected) without panicking (notion not touched)
        assert!(result.is_ok(), "expected success but got: {result:?}");
        assert_eq!(
            github_count.load(Ordering::SeqCst),
            1,
            "github should connect once"
        );
    }

    /// contains_mcp_tool returns true for a declared tool without connecting.
    #[tokio::test]
    async fn test_contains_tool_declared_returns_true() {
        let connect_count = Arc::new(AtomicUsize::new(0));
        let client = MockMcpClientInfra::new(vec![], connect_count.clone());

        let mut config = McpConfig::default();
        config.mcp_servers.insert(
            ServerName::from("github".to_string()),
            server_config_with_tools(vec!["search_repos"]),
        );

        let svc = make_service(config, client);
        let found = svc
            .contains_mcp_tool(&ToolName::new("mcp_github_tool_search_repos"))
            .await
            .unwrap();

        assert!(found, "expected declared tool to be found");
        assert_eq!(
            connect_count.load(Ordering::SeqCst),
            0,
            "no connection should occur"
        );
    }

    /// After a live connection, stale declared stubs are pruned (Task 1 fix).
    #[tokio::test]
    async fn test_stale_declared_stubs_pruned_after_live_connection() {
        let connect_count = Arc::new(AtomicUsize::new(0));
        // Server declares "old_tool" in config but live list returns "new_tool"
        let client = MockMcpClientInfra::new(vec!["new_tool"], connect_count.clone());

        let mut config = McpConfig::default();
        config.mcp_servers.insert(
            ServerName::from("srv".to_string()),
            server_config_with_tools(vec!["old_tool"]),
        );

        let svc = make_service(config, client);

        // Trigger live connection by calling the undeclared live tool (connect_all_pending)
        let call = ToolCallFull::new(ToolName::new("mcp_srv_tool_new_tool"));
        let _ = svc.execute_mcp(call).await;

        // After connection: old_tool stub should be gone, new_tool should be present
        let has_old = svc
            .contains_mcp_tool(&ToolName::new("mcp_srv_tool_old_tool"))
            .await
            .unwrap();
        let has_new = svc
            .contains_mcp_tool(&ToolName::new("mcp_srv_tool_new_tool"))
            .await
            .unwrap();

        assert!(!has_old, "stale declared stub should have been pruned");
        assert!(has_new, "live tool should be visible after connection");
    }

    /// Concurrent calls to connect the same server only trigger one actual connection.
    #[tokio::test]
    async fn test_concurrent_connect_server_idempotent() {
        let connect_count = Arc::new(AtomicUsize::new(0));
        let client = MockMcpClientInfra::new(vec!["list_prs"], connect_count.clone());

        let mut config = McpConfig::default();
        config.mcp_servers.insert(
            ServerName::from("github".to_string()),
            server_config_with_tools(vec!["list_prs"]),
        );

        let svc = Arc::new(make_service(config, client));

        let svc1 = svc.clone();
        let svc2 = svc.clone();
        let call1 = ToolCallFull::new(ToolName::new("mcp_github_tool_list_prs"));
        let call2 = ToolCallFull::new(ToolName::new("mcp_github_tool_list_prs"));

        let (r1, r2) = tokio::join!(svc1.execute_mcp(call1), svc2.execute_mcp(call2));

        assert!(r1.is_ok(), "first call should succeed: {r1:?}");
        assert!(r2.is_ok(), "second call should succeed: {r2:?}");

        // connect() should have been called exactly once despite the race
        assert_eq!(
            connect_count.load(Ordering::SeqCst),
            1,
            "connect() should be called exactly once"
        );
    }
}
