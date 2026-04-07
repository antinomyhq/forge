use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol as acp;
use forge_domain::{AgentId, ConversationId, ModelId};
use tokio::sync::{Mutex, Notify, mpsc};

use crate::Services;

use super::error::{Error, Result};

/// Maximum number of buffered session notifications before backpressure.
const NOTIFICATION_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone)]
pub(super) struct SessionState {
    pub conversation_id: ConversationId,
    pub agent_id: AgentId,
    /// Session-scoped model override. When set, prompts use this model
    /// instead of the global default.
    pub model_id: Option<ModelId>,
    pub cancel_notify: Option<Arc<Notify>>,
}

pub(crate) struct AcpAdapter<S> {
    pub(super) services: Arc<S>,
    pub(super) session_update_tx: mpsc::Sender<acp::SessionNotification>,
    pub(super) client_conn: Arc<Mutex<Option<Arc<acp::AgentSideConnection>>>>,
    sessions: Arc<Mutex<HashMap<String, SessionState>>>,
}

impl<S: Services> AcpAdapter<S> {
    /// Creates a new ACP adapter and returns the notification receiver.
    pub(crate) fn new(
        services: Arc<S>,
    ) -> (Self, mpsc::Receiver<acp::SessionNotification>) {
        let (tx, rx) = mpsc::channel(NOTIFICATION_CHANNEL_CAPACITY);
        let adapter = Self {
            services,
            session_update_tx: tx,
            client_conn: Arc::new(Mutex::new(None)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        };
        (adapter, rx)
    }

    pub(crate) async fn set_client_connection(&self, conn: Arc<acp::AgentSideConnection>) {
        *self.client_conn.lock().await = Some(conn);
    }

    pub(super) async fn store_session(&self, session_id: String, state: SessionState) {
        self.sessions.lock().await.insert(session_id, state);
    }

    /// Removes a session from the adapter. Currently unused but available
    /// for future session lifecycle management (TTL, explicit close).
    #[allow(dead_code)]
    pub(super) async fn remove_session(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }

    pub(super) async fn session_state(&self, session_id: &str) -> Result<SessionState> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| Error::Application(anyhow::anyhow!("Session not found")))
    }

    pub(super) async fn update_session_agent(
        &self,
        session_id: &str,
        agent_id: AgentId,
    ) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        let state = sessions
            .get_mut(session_id)
            .ok_or_else(|| Error::Application(anyhow::anyhow!("Session not found")))?;
        state.agent_id = agent_id;
        Ok(())
    }

    pub(super) async fn update_session_model(
        &self,
        session_id: &str,
        model_id: ModelId,
    ) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        let state = sessions
            .get_mut(session_id)
            .ok_or_else(|| Error::Application(anyhow::anyhow!("Session not found")))?;
        state.model_id = Some(model_id);
        Ok(())
    }

    pub(super) async fn set_cancel_notify(
        &self,
        session_id: &str,
        cancel_notify: Option<Arc<Notify>>,
    ) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        let state = sessions
            .get_mut(session_id)
            .ok_or_else(|| Error::Application(anyhow::anyhow!("Session not found")))?;
        state.cancel_notify = cancel_notify;
        Ok(())
    }

    pub(super) async fn cancel_session(&self, session_id: &str) -> bool {
        let notify = self
            .sessions
            .lock()
            .await
            .get(session_id)
            .and_then(|state| state.cancel_notify.clone());

        if let Some(notify) = notify {
            notify.notify_waiters();
            true
        } else {
            false
        }
    }

    pub(super) async fn ensure_session(
        &self,
        session_id: &str,
        conversation_id: ConversationId,
        agent_id: AgentId,
    ) -> SessionState {
        let mut sessions = self.sessions.lock().await;
        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionState {
                conversation_id,
                agent_id,
                model_id: None,
                cancel_notify: None,
            })
            .clone()
    }

    pub(super) fn send_notification(&self, notification: acp::SessionNotification) -> Result<()> {
        self.session_update_tx
            .try_send(notification)
            .map_err(|_| Error::Application(anyhow::anyhow!("Failed to send notification")))
    }
}
