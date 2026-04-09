//! Lazy MCP client that defers connection until a tool is actually called.
//!
//! During discovery (building the tool list for the system prompt) Forge only
//! needs to know *which* tools a server exposes, not to hold a live connection
//! to it. `LazyMcpClient` separates these two concerns:
//!
//! - **Discovery**: the client is constructed from config without any network
//!   I/O.  Tool names and schemas come either from statically-declared tools in
//!   the MCP config, or are left unknown until the first call.
//! - **Execution**: on the first `list()` or `call()`, the real
//!   `McpServerInfra::Client` is initialised via `connect()`.  Subsequent
//!   calls reuse the same underlying client.
//!
//! Thread-safety is guaranteed by [`tokio::sync::OnceCell`]: even if two
//! concurrent callers race to initialise the connection, only one
//! initialisation will run.

use std::collections::BTreeMap;
use std::sync::Arc;

use forge_app::{McpClientInfra, McpServerInfra};
use forge_domain::{McpServerConfig, ToolDefinition, ToolName, ToolOutput};
use tokio::sync::OnceCell;

/// A lazily-initialised MCP client.
///
/// Holds the configuration needed to connect to an MCP server and defers
/// the actual connection until [`list`] or [`call`] is first invoked.
pub(crate) struct LazyMcpClient<I: McpServerInfra> {
    config: McpServerConfig,
    env_vars: BTreeMap<String, String>,
    infra: Arc<I>,
    /// The real client, initialised on first use.
    inner: Arc<OnceCell<I::Client>>,
}

impl<I: McpServerInfra> Clone for LazyMcpClient<I> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            env_vars: self.env_vars.clone(),
            infra: self.infra.clone(),
            // Share the same OnceCell so all clones see the same connection.
            inner: self.inner.clone(),
        }
    }
}

impl<I: McpServerInfra> LazyMcpClient<I> {
    pub(crate) fn new(
        config: McpServerConfig,
        env_vars: BTreeMap<String, String>,
        infra: Arc<I>,
    ) -> Self {
        Self { config, env_vars, infra, inner: Arc::new(OnceCell::new()) }
    }

    /// Ensure the inner client is initialised and return a reference to it.
    async fn client(&self) -> anyhow::Result<&I::Client> {
        self.inner
            .get_or_try_init(|| async {
                self.infra
                    .connect(self.config.clone(), &self.env_vars)
                    .await
            })
            .await
    }

    /// Consume the lazy client and return the initialised inner client.
    ///
    /// Prefers taking sole ownership via `Arc::try_unwrap` when this is the
    /// last holder of the inner `Arc`.  When other holders still exist (e.g.,
    /// a clone kept alive in `pending_servers` at call time), it falls back to
    /// cloning the already-initialised inner value — the two resulting handles
    /// will share the same underlying transport.
    ///
    /// # Errors
    /// Returns an error if the inner client has not yet been initialised (i.e.,
    /// neither `list()` nor `call()` has been called).
    pub(crate) async fn into_inner(self) -> anyhow::Result<I::Client>
    where
        I::Client: Clone + Send + Sync + 'static,
    {
        // Take ownership of the Arc; if we hold the only reference, we can
        // unwrap it without Clone.  Otherwise clone the inner value.
        match Arc::try_unwrap(self.inner) {
            Ok(once_cell) => once_cell
                .into_inner()
                .ok_or_else(|| anyhow::anyhow!("LazyMcpClient: inner client not yet initialised")),
            Err(arc) => arc
                .get()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("LazyMcpClient: inner client not yet initialised")),
        }
    }
}

#[async_trait::async_trait]
impl<I: McpServerInfra + Send + Sync + 'static> McpClientInfra for LazyMcpClient<I> {
    /// List tools — connects on first call, reuses the connection thereafter.
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.client().await?.list().await
    }

    /// Execute a tool call — connects on first call, reuses thereafter.
    async fn call(
        &self,
        tool_name: &ToolName,
        input: serde_json::Value,
    ) -> anyhow::Result<ToolOutput> {
        self.client().await?.call(tool_name, input).await
    }
}
