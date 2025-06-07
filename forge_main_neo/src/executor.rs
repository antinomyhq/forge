use std::sync::Arc;

use forge_api::API;
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::{Command, CommandExecutor, Message};

pub struct ForgeCommandExecutor<A> {
    api: Arc<A>,
}

impl<A: API> ForgeCommandExecutor<A> {
    pub fn new(api: Arc<A>) -> Self {
        Self { api }
    }
}

#[async_trait::async_trait]
impl<A: API + Send + Sync + 'static> CommandExecutor for ForgeCommandExecutor<A> {
    async fn execute(&self, command: Command, tx: mpsc::Sender<Message>) -> anyhow::Result<()> {
        match command {
            Command::Suspend => todo!(),
            Command::ToggleMode(_) => todo!(),
            Command::UserMessage(request) => {
                let mut stream = self.api.chat(request).await?;
                while let Some(response) = stream.next().await {
                    tx.send(response?.into()).await?
                }
            }
            Command::InitConversation => {
                let workflow = self.api.read_workflow(None).await?;
                let conversation = self.api.init_conversation(workflow).await?;
                tx.send(Message::ConversationId(conversation.id)).await?
            }
            Command::Exit => {}
        }

        Ok(())
    }

    fn is_exit(&self, command: &Command) -> bool {
        matches!(command, Command::Exit)
    }
}
