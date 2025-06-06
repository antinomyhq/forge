use std::sync::Arc;

use forge_domain::Buffer;

use crate::infra::{ConsolePrintService as _, Infrastructure};
use crate::services::ConsoleService;
use crate::ConversationSessionManager;

pub struct ForgeConsoleService<I, S> {
    infra: Arc<I>,
    conversation_session_manager: Arc<S>,
}

impl<I, S> ForgeConsoleService<I, S> {
    pub fn new(infra: Arc<I>, conversation_session_manager: Arc<S>) -> Self {
        Self { infra, conversation_session_manager }
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure, S: ConversationSessionManager> ConsoleService
    for ForgeConsoleService<I, S>
{
    async fn print(&self, output: &str) -> anyhow::Result<()> {
        self.infra.console_print_service().print(output).await?;
        self.conversation_session_manager
            .buffer_update(Buffer::output(output))
            .await?;
        Ok(())
    }
}
