use crate::{FsCreateDirsService, FsReadService, FsWriteService, Infrastructure};
use anyhow::Context;
use bytes::Bytes;
use forge_domain::{Conversation, ConversationStore, EnvironmentService};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct ForgeConversationStoreService<I> {
    project_dir: PathBuf,
    infra: Arc<I>,
}

impl<I: Infrastructure> ForgeConversationStoreService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.environment_service().get_environment();
        let project_dir = env
            .base_path
            .join("conversations")
            .join(Self::hash(&env.cwd).to_string());

        Self { project_dir, infra }
    }
    fn hash(path: &PathBuf) -> u64 {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }
    fn conversation_path(&self) -> PathBuf {
        self.project_dir.join("conversation.json")
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> ConversationStore for ForgeConversationStoreService<I> {
    async fn load(&self) -> anyhow::Result<Option<Conversation>> {
        match self
            .infra
            .file_read_service()
            .read_utf8(&self.conversation_path())
            .await
        {
            Ok(convo) => {
                let conversation: Conversation = serde_json::from_str(&convo)?;
                Ok(Some(conversation))
            }
            Err(e) => {
                // Check if the error is due to the file not being found
                if e.downcast_ref::<std::io::Error>()
                    .map(|err| err.kind() == std::io::ErrorKind::NotFound)
                    .unwrap_or_default()
                {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn save(&self, conversation: &Conversation) -> anyhow::Result<()> {
        let json = serde_json::to_string(conversation)?;
        self.infra
            .create_dirs_service()
            .create_dirs(&self.project_dir)
            .await
            .ok();

        self.infra
            .file_write_service()
            .write(&self.conversation_path(), Bytes::from(json))
            .await
            .context("Failed to save conversation")
    }
}
