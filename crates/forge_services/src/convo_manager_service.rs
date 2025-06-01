use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use forge_domain::{Buffer, Conversation, ConversationSessionManager, EnvironmentService};
use futures::StreamExt;
use thiserror::__private::AsDisplay;

use crate::{
    BufferService, FileRemoveService, FsCreateDirsService, FsMetaService, FsReadService,
    FsWriteService, Infrastructure,
};

pub struct ForgeConversationSessionManager<I> {
    infra: Arc<I>,
    conversation_dir: PathBuf,
}

impl<I: Infrastructure> ForgeConversationSessionManager<I> {
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.environment_service().get_environment();

        let conversation_dir = env
            .conversations_dir()
            .join(Self::hash(&env.cwd).to_string());

        Self { infra, conversation_dir }
    }
    fn hash(cwd: &Path) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(cwd.as_display().to_string().as_bytes());
        hasher.finish()
    }
    fn state_path(&self) -> PathBuf {
        self.conversation_dir.join("state.jsonl")
    }
    fn conversation_path(&self) -> PathBuf {
        self.conversation_dir.join("conversation.json")
    }
    async fn create_dir(&self) -> anyhow::Result<()> {
        if !self
            .infra
            .file_meta_service()
            .exists(&self.conversation_dir)
            .await
            .unwrap_or_default()
        {
            self.infra
                .create_dirs_service()
                .create_dirs(&self.conversation_dir)
                .await?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> ConversationSessionManager for ForgeConversationSessionManager<I> {
    async fn load(&self) -> anyhow::Result<Conversation> {
        let convo_path = self.conversation_path();
        let conversation =
            serde_json::from_slice(&self.infra.file_read_service().read(&convo_path).await?)?;
        Ok(conversation)
    }

    async fn state(&self, n: usize) -> anyhow::Result<Vec<Buffer>> {
        let state_path = self.state_path();
        let buffer = self.infra.buffer_service().read(&state_path).await?;
        let buffer = buffer
            .take(n)
            .filter_map(|v| async { v.ok() })
            .collect::<Vec<_>>()
            .await;
        Ok(buffer)
    }

    async fn buffer_update(&self, state: Buffer) -> anyhow::Result<()> {
        self.create_dir().await?;

        let buffer_path = self.state_path();
        self.infra
            .buffer_service()
            .write(&buffer_path, state)
            .await?;
        Ok(())
    }

    async fn conversation_update(&self, conversation: &Conversation) -> anyhow::Result<()> {
        self.create_dir().await?;

        let conversation_path = self.conversation_path();
        self.infra
            .file_write_service()
            .write(
                &conversation_path,
                Bytes::from(serde_json::to_string(&conversation)?),
            )
            .await?;
        Ok(())
    }

    async fn clear(&self) -> anyhow::Result<()> {
        self.infra
            .file_remove_service()
            .remove(&self.conversation_dir)
            .await?;
        Ok(())
    }
}
