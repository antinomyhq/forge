use std::sync::Arc;

use forge_domain::{Agent, ContextMessage, Conversation};
use forge_template::Element;

use crate::FsReadService;

/// Service responsible for detecting externally changed files and rendering
/// notifications
pub struct ChangedFiles<S> {
    services: Arc<S>,
    agent: Agent,
}

impl<S> ChangedFiles<S> {
    /// Creates a new ChangedFiles
    pub fn new(services: Arc<S>, agent: Agent) -> Self {
        Self { services, agent }
    }
}

impl<S: FsReadService> ChangedFiles<S> {
    /// Detects externally changed files and renders a notification if changes
    /// are found. Updates file hashes in conversation metrics to prevent
    /// duplicate notifications.
    pub async fn update_file_stats(&self, mut conversation: Conversation) -> Conversation {
        use crate::file_tracking::FileChangeDetector;
        let changes = FileChangeDetector::new(self.services.clone())
            .detect(&conversation.metrics)
            .await;

        if changes.is_empty() {
            return conversation;
        }

        // Update file hashes to prevent duplicate notifications
        let mut updated_metrics = conversation.metrics.clone();
        for change in &changes {
            if let Some(path_str) = change.path.to_str()
                && let Some(metrics) = updated_metrics.file_operations.get_mut(path_str)
            {
                // Update the file hash
                metrics.content_hash = change.content_hash.clone();
            }
        }
        conversation.metrics = updated_metrics;

        let file_elements: Vec<Element> = changes
            .iter()
            .map(|change| Element::new("file").text(change.path.display().to_string()))
            .collect();

        let notification = Element::new("information")
            .append(
                Element::new("critical")
                    .text("The following files have been modified externally. Please re-read them if its relevant for the task."),
            )
            .append(Element::new("files").append(file_elements))
            .to_string();

        let context = conversation.context.take().unwrap_or_default();
        conversation = conversation.context(
            context.add_message(ContextMessage::user(notification, self.agent.model.clone())),
        );

        conversation
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_domain::{
        Agent, AgentId, Context, Conversation, ConversationId, FileOperation, Metrics, ModelId,
        ToolKind,
    };
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::services::Content;
    use crate::{FsReadService, ReadOutput};

    #[derive(Clone, Default)]
    struct TestServices {
        files: HashMap<String, String>,
    }

    #[async_trait::async_trait]
    impl FsReadService for TestServices {
        async fn read(
            &self,
            path: String,
            _: Option<u64>,
            _: Option<u64>,
        ) -> anyhow::Result<ReadOutput> {
            self.files
                .get(&path)
                .map(|content| ReadOutput {
                    content: Content::File(content.clone()),
                    start_line: 1,
                    end_line: 1,
                    total_lines: 1,
                })
                .ok_or_else(|| anyhow::anyhow!(std::io::Error::from(std::io::ErrorKind::NotFound)))
        }
    }

    fn fixture(
        files: HashMap<String, String>,
        tracked_files: HashMap<String, Option<String>>,
    ) -> (ChangedFiles<TestServices>, Conversation) {
        let services = Arc::new(TestServices { files });
        let agent = Agent::new(AgentId::new("test")).model(ModelId::new("test-model"));
        let changed_files = ChangedFiles::new(services, agent);

        let mut metrics = Metrics::new();
        for (path, hash) in tracked_files {
            metrics
                .file_operations
                .insert(path, FileOperation::new(ToolKind::Write).content_hash(hash));
        }

        let conversation = Conversation::new(ConversationId::generate()).metrics(metrics);

        (changed_files, conversation)
    }

    #[tokio::test]
    async fn test_no_changes_detected() {
        let content = "hello world";
        let hash = crate::compute_hash(content);

        let (service, mut conversation) = fixture(
            [("/test/file.txt".into(), content.into())].into(),
            [("/test/file.txt".into(), Some(hash))].into(),
        );

        conversation.context = Some(Context::default().add_message(ContextMessage::user(
            "Hey, there!",
            Some(ModelId::new("test")),
        )));

        let actual = service.update_file_stats(conversation.clone()).await;

        assert_eq!(actual.context.clone().unwrap_or_default().messages.len(), 1);
        assert_eq!(actual.context, conversation.context);
    }

    #[tokio::test]
    async fn test_changes_detected_adds_notification() {
        let old_hash = crate::compute_hash("old content");
        let new_content = "new content";

        let (service, conversation) = fixture(
            [("/test/file.txt".into(), new_content.into())].into(),
            [("/test/file.txt".into(), Some(old_hash))].into(),
        );

        let actual = service.update_file_stats(conversation).await;

        let messages = &actual.context.unwrap().messages;
        assert_eq!(messages.len(), 1);
        let message = messages[0].content().unwrap().to_string();
        assert!(message.contains("/test/file.txt"));
        assert!(message.contains("modified externally"));
    }

    #[tokio::test]
    async fn test_updates_content_hash() {
        let old_hash = crate::compute_hash("old content");
        let new_content = "new content";
        let new_hash = crate::compute_hash(new_content);

        let (service, conversation) = fixture(
            [("/test/file.txt".into(), new_content.into())].into(),
            [("/test/file.txt".into(), Some(old_hash))].into(),
        );

        let actual = service.update_file_stats(conversation).await;

        let updated_hash = actual
            .metrics
            .file_operations
            .get("/test/file.txt")
            .and_then(|m| m.content_hash.clone());

        assert_eq!(updated_hash, Some(new_hash));
    }

    #[tokio::test]
    async fn test_multiple_files_changed() {
        let (service, conversation) = fixture(
            [
                ("/test/file1.txt".into(), "new 1".into()),
                ("/test/file2.txt".into(), "new 2".into()),
            ]
            .into(),
            [
                ("/test/file1.txt".into(), Some(crate::compute_hash("old 1"))),
                ("/test/file2.txt".into(), Some(crate::compute_hash("old 2"))),
            ]
            .into(),
        );

        let actual = service.update_file_stats(conversation).await;

        let message = actual.context.unwrap().messages[0]
            .content()
            .unwrap()
            .to_string();
        assert!(message.contains("/test/file1.txt"));
        assert!(message.contains("/test/file2.txt"));
    }
}
