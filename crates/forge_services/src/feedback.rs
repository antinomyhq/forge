use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use forge_domain::FeedbackService;
use forge_fs::ForgeFS;
use tracing::warn;

use crate::feedback_settings::FeedbackSettings;
use crate::{FsMetaService, FsReadService, FsWriteService, Infrastructure};

pub struct ForgeFeedbackService<F> {
    settings_path: PathBuf,
    infra: Arc<F>,
}

impl<F: Infrastructure> ForgeFeedbackService<F> {
    pub fn new(base_path: PathBuf, infra: Arc<F>) -> Self {
        let settings_path = base_path.join("settings").join("feedback.json");
        Self { settings_path, infra }
    }

    /// Load feedback settings from the file, returning default settings if any
    /// error occurs
    async fn load_settings(&self) -> FeedbackSettings {
        if let Ok(exist) = self
            .infra
            .file_meta_service()
            .exists(&self.settings_path)
            .await
        {
            if !exist {
                return FeedbackSettings::default();
            }
        }
        // Read the settings file
        match self
            .infra
            .file_read_service()
            .read_utf8(&self.settings_path)
            .await
        {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(settings) => settings,
                Err(e) => {
                    warn!(error = %e, "Failed to parse feedback settings");
                    FeedbackSettings::default()
                }
            },
            Err(e) => {
                warn!(error = %e, "Failed to read feedback settings");
                FeedbackSettings::default()
            }
        }
    }

    /// Save the provided settings to disk
    async fn save_settings(&self, settings: &FeedbackSettings) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(settings)?;
        if let Some(parent) = self.settings_path.parent() {
            if let Ok(exist) = self.infra.file_meta_service().exists(parent).await {
                if !exist {
                    ForgeFS::create_dir_all(parent).await?
                }
            }
        }
        let content = content.as_bytes();
        let bytes = Bytes::copy_from_slice(content);
        self.infra
            .file_write_service()
            .write(&self.settings_path, bytes)
            .await
    }
}

// Implement the FeedbackService trait for our async service
#[async_trait::async_trait]
impl<F: Infrastructure> FeedbackService for ForgeFeedbackService<F> {
    // Check if feedback should be shown - always reads from disk
    async fn should_show_feedback(&self) -> anyhow::Result<bool> {
        // Read settings from disk
        let settings = self.load_settings().await;

        // Return whether feedback should be shown
        Ok(settings.should_show_feedback())
    }

    // Update the last shown timestamp
    async fn update_last_shown(&self) -> anyhow::Result<()> {
        let mut settings = self.load_settings().await;

        // Update the timestamp
        settings.update_last_shown();

        // Save the updated settings back to disk
        self.save_settings(&settings).await
    }
}
