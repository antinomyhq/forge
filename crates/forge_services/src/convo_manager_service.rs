use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use forge_app::{ConversationSessionManager, EnvironmentService};
use forge_domain::{Buffer, Conversation};
use serde::{Deserialize, Serialize};
use thiserror::__private::AsDisplay;
use tokio::sync::RwLock;

use crate::{
    BufferService, FileRemoveService, FsCreateDirsService, FsMetaService, FsReadService,
    FsWriteService, Infrastructure,
};

pub struct ForgeConversationSessionManager<I> {
    infra: Arc<I>,
    session_store: PathBuf,
    project_dir: PathBuf,
    session_id: Arc<RwLock<Option<String>>>,
}

#[derive(Serialize, Deserialize)]
struct SessionIdStore {
    id: String,
}

impl<I: Infrastructure> ForgeConversationSessionManager<I> {
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.environment_service().get_environment();
        let project_dir = env
            .conversations_dir()
            .join(Self::hash(&env.cwd).to_string());

        Self {
            infra,
            session_store: project_dir.join("latest.json"),
            project_dir,
            session_id: Arc::new(Default::default()),
        }
    }
    fn hash(cwd: &Path) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(cwd.as_display().to_string().as_bytes());
        hasher.finish()
    }

    // creates session path, i.e.
    // ~/forge/conversations/<hash of cwd>/<session/conversation id>
    async fn session_path(&self) -> anyhow::Result<PathBuf> {
        Ok(self.project_dir.join(self.session_id().await?))
    }
    async fn state_path(&self) -> anyhow::Result<PathBuf> {
        Ok(self.session_path().await?.join("state.jsonl"))
    }
    async fn conversation_path(&self) -> anyhow::Result<PathBuf> {
        Ok(self.session_path().await?.join("conversation.json"))
    }
    async fn create_dir(&self) -> anyhow::Result<()> {
        let session_dir = self.session_path().await?;
        if !self
            .infra
            .file_meta_service()
            .exists(&session_dir)
            .await
            .unwrap_or_default()
        {
            self.infra
                .create_dirs_service()
                .create_dirs(&session_dir)
                .await?;
        }

        Ok(())
    }

    async fn session_id(&self) -> anyhow::Result<String> {
        if let Some(session_id) = self.session_id.read().await.clone() {
            return Ok(session_id);
        }

        let content = self
            .infra
            .file_read_service()
            .read_utf8(&self.session_store)
            .await?;
        let session_store = serde_json::from_str::<SessionIdStore>(&content)?;
        self.update_session_id(session_store.id.clone()).await?;

        Ok(session_store.id)
    }

    async fn update_session_id(&self, conversation_id: String) -> anyhow::Result<()> {
        self.infra
            .create_dirs_service()
            .create_dirs(&self.project_dir)
            .await
            .ok();
        self.infra
            .file_write_service()
            .write(
                self.session_store.as_path(),
                Bytes::from(serde_json::to_string(&SessionIdStore {
                    id: conversation_id.clone(),
                })?),
            )
            .await?;
        self.session_id.write().await.replace(conversation_id);
        Ok(())
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> ConversationSessionManager for ForgeConversationSessionManager<I> {
    async fn load(&self) -> anyhow::Result<Conversation> {
        let convo_path = self.conversation_path().await?;
        let conversation =
            serde_json::from_slice(&self.infra.file_read_service().read(&convo_path).await?)?;
        Ok(conversation)
    }

    async fn state(&self, buffer_size: usize) -> anyhow::Result<Vec<Buffer>> {
        let state_path = self.state_path().await?;
        let buffer = self
            .infra
            .buffer_service()
            .read_last(&state_path, usize::MAX)
            .await?;

        let mut result = Vec::with_capacity(buffer.len());
        let mut total_size = 0;

        for (i, buffer) in buffer.into_iter().flatten().rev().enumerate() {
            let content_size = buffer.content.len();
            if total_size + content_size > buffer_size {
                break;
            }
            total_size += content_size;
            result[i] = buffer;
        }

        Ok(result)
    }

    async fn buffer_update(&self, state: Buffer) -> anyhow::Result<()> {
        self.create_dir().await.ok();

        let buffer_path = self.state_path().await?;
        self.infra
            .buffer_service()
            .write(&buffer_path, state)
            .await?;
        Ok(())
    }

    async fn conversation_update(&self, conversation: &Conversation) -> anyhow::Result<()> {
        self.update_session_id(conversation.id.to_string()).await?;
        self.create_dir().await.ok();

        let conversation_path = self.conversation_path().await?;
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
            .remove(&self.session_store)
            .await
            .ok();
        self.session_id.write().await.take();
        Ok(())
    }
}
