use std::sync::Arc;

use forge_domain::{Agent, ContextMessage, Conversation, Template};

use crate::Services;
use crate::agent::AgentService;

/// Service responsible for detecting externally changed files and rendering
/// notifications
pub struct ChangedFiles<S> {
    services: Arc<S>,
    agent: Agent,
}

impl<S: Services> ChangedFiles<S> {
    /// Creates a new ChangedFiles
    pub fn new(services: Arc<S>, agent: Agent) -> Self {
        Self { services, agent }
    }

    /// Detects externally changed files and renders a notification if changes
    /// are found. Updates file hashes in conversation metrics to prevent
    /// duplicate notifications.
    pub async fn detect_externally_modified_files(&self, mut conversation: Conversation) -> Conversation {
        use crate::file_tracking::FileChangeDetector;
        let mut context = conversation.context.take().unwrap_or_default();
        let changes = FileChangeDetector::new(self.services.clone())
            .detect(&conversation.metrics)
            .await;

        if changes.is_empty() {
            return conversation;
        }

        // Update file hashes to prevent duplicate notifications
        for change in &changes {
            if let Some(path_str) = change.path.to_str()
                && let Some(metrics) = conversation.metrics.files_changed.get_mut(path_str)
            {
                metrics.file_hash = change.file_hash.clone();
            }
        }

        let file_paths: Vec<String> = changes
            .iter()
            .map(|change| change.path.display().to_string())
            .collect();

        if let Ok(rendered_prompt) = self
            .services
            .render(
                Template::new("{{> forge-file-changes-notification.md }}"),
                &serde_json::json!({ "files": file_paths }),
            )
            .await
        {
            context = context.add_message(ContextMessage::user(
                rendered_prompt,
                self.agent.model.clone(),
            ))
        }

        conversation.context(context)
    }
}
