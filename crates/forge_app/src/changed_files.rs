use std::sync::Arc;

use forge_domain::Conversation;

use crate::Services;
use crate::agent::AgentService;

/// Service responsible for detecting externally changed files and rendering
/// notifications
pub struct ChangedFiles<S> {
    services: Arc<S>,
}

impl<S: Services> ChangedFiles<S> {
    /// Creates a new ChangedFiles
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Detects externally changed files and renders a notification if changes
    /// are found. Updates file hashes in conversation metrics to prevent
    /// duplicate notifications.
    pub async fn detect_and_render(&self, conversation: &mut Conversation) -> Option<String> {
        use crate::file_tracking::FileChangeDetector;

        let changes = FileChangeDetector::new(self.services.clone())
            .detect(&conversation.metrics)
            .await;

        if changes.is_empty() {
            return None;
        }

        // Update file hashes to prevent duplicate notifications
        for change in &changes {
            if let Some(path_str) = change.path.to_str()
                && let Some(metrics) = conversation.metrics.files_changed.get_mut(path_str) {
                    metrics.file_hash = change.file_hash.clone();
                }
        }

        let file_paths: Vec<String> = changes
            .iter()
            .map(|change| change.path.display().to_string())
            .collect();

        self.services
            .render(
                "{{> forge-file-changes-notification.md }}",
                &serde_json::json!({ "files": file_paths }),
            )
            .await
            .ok()
    }
}
