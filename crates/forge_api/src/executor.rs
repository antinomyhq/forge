use std::sync::Arc;


use forge_domain::{AgentMessage, App, ChatRequest, ChatResponse, ContextMessage, ConversationId, ConversationService, Error, Event, Orchestrator, Role};

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
        let conversation = self.app.conversation_service()
            .get(&conversation_id)
            .await?
            .ok_or(Error::ConversationNotFound(conversation_id.clone()))?;
        let agent_with_context = conversation.state.iter()
            .find_map(|(agent_id, state)| state.context.as_ref().map(|context| (agent_id, context)));

        if let Some((_agent_id, context)) = agent_with_context {
            let last_user_message = context.messages.iter().rev()
                .find_map(|msg| match msg {
                    ContextMessage::ContentMessage(content_msg) if content_msg.role == Role::User => {
                        Some(content_msg.content.clone())
                    }
                    _ => None
                })
                .ok_or(anyhow::anyhow!("no user message found"))?;

            let event = Event::new("user_task_update", last_user_message);
            let chat_request = ChatRequest::new(event, conversation_id);
            self.chat(chat_request).await
        } else {
            Err(anyhow::anyhow!("no agent with context found in conversation"))
        }
    }
}
