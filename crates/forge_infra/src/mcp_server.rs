use std::collections::BTreeMap;
use std::sync::Arc;

use forge_app::{ElicitationDispatcher, McpServerInfra};
use forge_domain::{Environment, McpServerConfig};
use tokio::sync::OnceCell;

use crate::mcp_client::ForgeMcpClient;

/// Factory for [`ForgeMcpClient`] instances that owns a shared,
/// late-init [`ElicitationDispatcher`] slot.
///
/// Wave F-2 introduces the dispatcher plumbing: `ForgeMcpServer` is
/// created during [`crate::ForgeInfra::new`] (before the
/// `ForgeServices` aggregate exists, so the dispatcher doesn't yet
/// exist either), then wired up via
/// [`ForgeMcpServer::set_elicitation_dispatcher`] from
/// `forge_api::ForgeAPI::init` once the services are built. Each
/// subsequent `connect` call threads a clone of the shared
/// `OnceCell` into the constructed [`ForgeMcpClient`] so the client's
/// `ForgeMcpHandler` can look up the dispatcher lazily at
/// `.serve(transport)` time.
///
/// Using [`tokio::sync::OnceCell`] instead of `std::sync::OnceLock`
/// is deliberate: the dispatcher needs to be shared across an
/// `Arc<ForgeMcpServer>`-style clone graph, and `OnceCell` composes
/// cleanly with async contexts (the `set` / `get` APIs are sync, but
/// it lives inside structs that may be held across `.await` points).
#[derive(Clone, Default)]
pub struct ForgeMcpServer {
    elicitation_dispatcher: Arc<OnceCell<Arc<dyn ElicitationDispatcher>>>,
}

impl ForgeMcpServer {
    /// Create a new server factory with an empty dispatcher slot.
    /// The slot is populated later via
    /// [`ForgeMcpServer::set_elicitation_dispatcher`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Populate the shared elicitation dispatcher slot. First call
    /// wins — subsequent calls are silently ignored per the
    /// [`OnceCell`] contract. Called from `forge_api::ForgeAPI::init`
    /// immediately after the `ForgeServices` aggregate is
    /// constructed, before any MCP server connections are initiated.
    pub fn set_elicitation_dispatcher(&self, dispatcher: Arc<dyn ElicitationDispatcher>) {
        let _ = self.elicitation_dispatcher.set(dispatcher);
    }
}

#[async_trait::async_trait]
impl McpServerInfra for ForgeMcpServer {
    type Client = ForgeMcpClient;

    async fn connect(
        &self,
        server_name: &str,
        config: McpServerConfig,
        env_vars: &BTreeMap<String, String>,
        environment: &Environment,
    ) -> anyhow::Result<Self::Client> {
        Ok(ForgeMcpClient::new(
            server_name.to_string(),
            config,
            env_vars,
            environment.clone(),
            self.elicitation_dispatcher.clone(),
        ))
    }
}
