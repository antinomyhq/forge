use std::sync::Arc;

use forge_domain::{Agent, ContextMessage, Conversation, Template};

use crate::{FsReadService, TemplateService};

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

impl<S: FsReadService + TemplateService> ChangedFiles<S> {
    /// Detects externally changed files and renders a notification if changes
    /// are found. Updates file hashes in conversation metrics to prevent
    /// duplicate notifications.
    pub async fn detect_externally_modified_files(
        &self,
        mut conversation: Conversation,
    ) -> Conversation {
        use crate::file_tracking::FileChangeDetector;
        let mut context = conversation.context.take().unwrap_or_default();
        let changes = FileChangeDetector::new(self.services.clone())
            .detect(&conversation.metrics)
            .await;

        if changes.is_empty() {
            return conversation.context(context);
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
            .render_template(
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_domain::{
        Agent, AgentId, Context, Conversation, ConversationId, FileChangeMetrics, Metrics, ModelId,
        Template,
    };
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::services::Content;
    use crate::{FsReadService, ReadOutput, TemplateService};

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

    #[async_trait::async_trait]
    impl TemplateService for TestServices {
        async fn register_template(&self, _: std::path::PathBuf) -> anyhow::Result<()> {
            Ok(())
        }

        async fn render_template<V: serde::Serialize + Send + Sync>(
            &self,
            _: Template<V>,
            object: &V,
        ) -> anyhow::Result<String> {
            let json = serde_json::to_value(object)?;
            let files = json
                .get("files")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| f.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            Ok(format!("Files changed: {}", files))
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
                .files_changed
                .insert(path, FileChangeMetrics::new(hash));
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

        let actual = service
            .detect_externally_modified_files(conversation.clone())
            .await;

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

        let actual = service.detect_externally_modified_files(conversation).await;

        let messages = &actual.context.unwrap().messages;
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].content().unwrap().to_string(),
            "Files changed: /test/file.txt"
        );
    }

    #[tokio::test]
    async fn test_updates_file_hash() {
        let old_hash = crate::compute_hash("old content");
        let new_content = "new content";
        let new_hash = crate::compute_hash(new_content);

        let (service, conversation) = fixture(
            [("/test/file.txt".into(), new_content.into())].into(),
            [("/test/file.txt".into(), Some(old_hash))].into(),
        );

        let actual = service.detect_externally_modified_files(conversation).await;

        let updated_hash = actual
            .metrics
            .files_changed
            .get("/test/file.txt")
            .unwrap()
            .file_hash
            .clone();

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

        let actual = service.detect_externally_modified_files(conversation).await;

        let message = actual.context.unwrap().messages[0]
            .content()
            .unwrap()
            .to_string();
        assert!(message.contains("/test/file1.txt"));
        assert!(message.contains("/test/file2.txt"));
    }
}
