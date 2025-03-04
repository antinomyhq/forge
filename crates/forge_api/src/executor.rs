use std::sync::Arc;


use forge_app::{EnvironmentService, Infrastructure};
use forge_domain::{
    AgentMessage, App, ChatRequest, ChatResponse, ConversationId, ConversationService, Error,
    Event, Orchestrator, SystemContext, ToolService,
};

use forge_stream::MpscStream;

pub struct ForgeExecutorService<F> {
    app: Arc<F>,
}
impl<F: App> ForgeExecutorService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { app: infra }
    }
}

impl<F: App> ForgeExecutorService<F> {
    pub async fn chat(
        &self,
        request: ChatRequest,
    ) -> anyhow::Result<MpscStream<anyhow::Result<AgentMessage<ChatResponse>>>> {
        let app = self.app.clone();

        Ok(MpscStream::spawn(move |tx| async move {
            let tx = Arc::new(tx);
            let orch = Orchestrator::new(app, request.conversation_id, Some(tx.clone()));

            match orch.dispatch(&request.event).await {
                Ok(_) => {}
                Err(err) => tx.send(Err(err)).await.unwrap(),
            }
        }))
    }

    pub async fn retry(
        &self,
        conversation_id: ConversationId,
    ) -> anyhow::Result<MpscStream<anyhow::Result<AgentMessage<ChatResponse>>>> {
        let conversation = self.infra.conversation_service()
            .get(&conversation_id)
            .await?
            .ok_or(Error::ConversationNotFound(conversation_id.clone()))?;
        let last_user_message = conversation
            .rfind_event(Event::USER_TASK_UPDATE)
            .or_else(|| conversation.rfind_event(Event::USER_TASK_INIT))
            .ok_or(anyhow::anyhow!("No user message found in the conversation"))?;
        let chat_request = ChatRequest::new(last_user_message.value.clone(), conversation_id);
        self.chat(chat_request).await
    }
}
