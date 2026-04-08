//! Core ACP protocol adapter

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol as acp;
use forge_domain::{ConversationId, SessionId};
use tokio::sync::{mpsc, Mutex};

use crate::{Services, SessionOrchestrator};

use super::error::{Error, Result};

/// Internal ACP protocol adapter
///
/// This adapter is responsible for:
/// - Translating ACP types ↔ Domain types
/// - Managing ACP session ID mapping
/// - Delegating business logic to services
/// - Sending notifications to the IDE
pub(crate) struct AcpAdapter<S> {
    /// Services for direct access
    pub(super) services: Arc<S>,
    /// Session orchestrator for coordinating session operations
    pub(super) session_orchestrator: SessionOrchestrator<S>,
    /// Channel for sending session notifications to the client
    pub(super) session_update_tx: mpsc::UnboundedSender<acp::SessionNotification>,
    /// Client connection for making RPC calls to the IDE client
    /// Used for requesting user permission during prompt execution
    pub(super) client_conn: Arc<Mutex<Option<Arc<acp::AgentSideConnection>>>>,
    /// Mapping from ACP session IDs to domain session IDs
    /// This is the only session-related state that should remain in the adapter
    pub(super) acp_to_domain_session: RefCell<HashMap<String, SessionId>>,
}

impl<S: Services> AcpAdapter<S> {
    /// Creates a new ACP adapter
    pub(crate) fn new(
        services: Arc<S>,
        session_update_tx: mpsc::UnboundedSender<acp::SessionNotification>,
    ) -> Self {
        let session_orchestrator = SessionOrchestrator::new(services.clone());
        Self {
            services,
            session_orchestrator,
            session_update_tx,
            client_conn: Arc::new(Mutex::new(None)),
            acp_to_domain_session: RefCell::new(HashMap::new()),
        }
    }

    /// Sets the client connection for making RPC calls to the IDE
    ///
    /// This must be called after creating the agent to enable user interaction
    /// features like requesting permission to continue after failures.
    pub(crate) async fn set_client_connection(&self, conn: Arc<acp::AgentSideConnection>) {
        *self.client_conn.lock().await = Some(conn);
    }

    /// Converts an ACP session ID to a Forge conversation ID
    pub(super) async fn to_conversation_id(
        &self,
        session_id: &acp::SessionId,
    ) -> Result<ConversationId> {
        let session_key = session_id.0.as_ref().to_string();

        // Get the domain session ID
        let domain_session_id = self
            .acp_to_domain_session
            .borrow()
            .get(&session_key)
            .copied()
            .ok_or_else(|| Error::Application(anyhow::anyhow!("Session not found")))?;

        // Get session context from SessionService
        use crate::SessionService as _;
        let session_context = self
            .services
            .session_service()
            .get_session_context(&domain_session_id)
            .await
            .map_err(Error::Application)?;

        Ok(session_context.state.conversation_id)
    }

    /// Sends a session notification to the client
    pub(super) fn send_notification(&self, notification: acp::SessionNotification) -> Result<()> {
        self.session_update_tx
            .send(notification)
            .map_err(|_| Error::Application(anyhow::anyhow!("Failed to send notification")))
    }
}
