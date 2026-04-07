//! Stub implementation of the Agent Client Protocol (ACP) server for Forge.
//!
//! This crate exposes a JSON-RPC server over stdio that implements the ACP specification.
//! The server is invoked via `forge acp` and communicates with clients using the
//! `agent-client-protocol` crate.

use agent_client_protocol::{
    AgentCapabilities, AgentSideConnection, AuthenticateRequest, AuthenticateResponse,
    AuthMethod, AuthMethodAgent, CancelNotification, Implementation, InitializeRequest,
    InitializeResponse, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
    SessionId, StopReason,
};
use futures::future::LocalBoxFuture;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// The stub ACP agent that implements the `Agent` trait with minimal no-op responses.
///
/// This is a placeholder implementation that accepts all protocol messages and responds
/// with empty/default values. The actual agent logic will be wired in separately.
struct ForgeAcpAgent;

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Agent for ForgeAcpAgent {
    /// Responds with the forge agent capabilities and negotiated protocol version.
    async fn initialize(&self, args: InitializeRequest) -> agent_client_protocol::Result<InitializeResponse> {
        let agent_info = Implementation::new("forge", env!("CARGO_PKG_VERSION"));
        let auth_methods = vec![AuthMethod::Agent(AuthMethodAgent::new("agent", "Forge"))];
        Ok(InitializeResponse::new(args.protocol_version)
            .agent_capabilities(AgentCapabilities::new())
            .agent_info(agent_info)
            .auth_methods(auth_methods))
    }

    /// Accepts all authentication attempts unconditionally.
    async fn authenticate(&self, _args: AuthenticateRequest) -> agent_client_protocol::Result<AuthenticateResponse> {
        Ok(AuthenticateResponse::new())
    }

    /// Creates a new ACP session and returns a fresh session ID.
    async fn new_session(&self, _args: NewSessionRequest) -> agent_client_protocol::Result<NewSessionResponse> {
        let id = uuid::Uuid::new_v4().to_string();
        Ok(NewSessionResponse::new(SessionId::new(id)))
    }

    /// Processes a prompt and returns a stop-reason of end-of-turn (stub).
    async fn prompt(&self, _args: PromptRequest) -> agent_client_protocol::Result<PromptResponse> {
        Ok(PromptResponse::new(StopReason::EndTurn))
    }

    /// Handles session cancellation notifications (no-op in stub).
    async fn cancel(&self, _args: CancelNotification) -> agent_client_protocol::Result<()> {
        Ok(())
    }
}

/// Starts the ACP JSON-RPC server, reading from stdin and writing to stdout.
///
/// Sets up an agent-side ACP connection over stdio and blocks until the connection
/// is closed or an I/O error occurs.
///
/// A `tokio::task::LocalSet` is created internally so that the `agent-client-protocol`
/// library can call `spawn_local` for its internal `!Send` coroutines, which is required
/// even when running under a multi-thread tokio runtime.
pub async fn run() -> anyhow::Result<()> {
    let local = tokio::task::LocalSet::new();
    local.run_until(run_inner()).await
}

/// Inner async body of [`run`], executed within a [`tokio::task::LocalSet`] context.
async fn run_inner() -> anyhow::Result<()> {
    let stdin = tokio::io::stdin().compat();
    let stdout = tokio::io::stdout().compat_write();

    let (_conn, io_task): (AgentSideConnection, _) = AgentSideConnection::new(
        ForgeAcpAgent,
        stdout,
        stdin,
        |fut: LocalBoxFuture<'static, ()>| {
            tokio::task::spawn_local(fut);
        },
    );

    io_task.await.map_err(|e| anyhow::anyhow!("ACP IO error: {e}"))
}
