use std::sync::Arc;

use forge_domain::{Agent, ContextMessage, Conversation, Role, TextMessage};
use forge_template::Element;

use crate::utils::format_display_path;
use crate::{ContextEngineService, EnvironmentService, FsReadService};

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

impl<S: FsReadService + EnvironmentService + ContextEngineService + 'static> ChangedFiles<S> {
    /// Detects externally changed files and renders a notification if changes
    /// are found. Updates file hashes in conversation metrics to prevent
    /// duplicate notifications.
    pub async fn handle_external_changes(&self, mut conversation: Conversation) -> Conversation {
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

        let cwd = self.services.get_environment().cwd;
        let file_elements: Vec<Element> = changes
            .iter()
            .map(|change| {
                let display_path = format_display_path(&change.path, &cwd);
                Element::new("file").text(display_path)
            })
            .collect();

        let notification = Element::new("information")
            .append(
                Element::new("critical")
                    .text("The following files have been modified externally. Please re-read them if its relevant for the task."),
            )
            .append(Element::new("files").append(file_elements))
            .to_string();

        let context = conversation.context.take().unwrap_or_default();

        let message = TextMessage::new(Role::User, notification)
            .droppable(true)
            .model(self.agent.model.clone());

        conversation = conversation.context(context.add_message(ContextMessage::from(message)));

        // Re-index the codebase if external changes were detected and codebase is
        // already indexed
        let services = self.services.clone();
        let batch_size = self.services.get_environment().sync_batch_size;
        tokio::spawn(async move {
            if services.is_indexed(&cwd).await.unwrap_or(false) {
                tracing::info!("Re-indexing codebase after detecting external file changes");
                let _ = services.sync_codebase(cwd.clone(), batch_size).await;
            }
        });

        conversation
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    use forge_domain::{
        Agent, AgentId, Context, Conversation, ConversationId, Environment, FileOperation, Metrics,
        ModelId, ProviderId, ToolKind,
    };
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::services::Content;
    use crate::{EnvironmentService, FsReadService, ReadOutput, compute_hash};

    #[derive(Default)]
    struct TestServices {
        files: HashMap<String, String>,
        cwd: Option<PathBuf>,
        indexed: bool,
        sync_called: AtomicBool,
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
                .map(|content| {
                    let hash = compute_hash(content);
                    ReadOutput {
                        content: Content::file(content.clone()),
                        start_line: 1,
                        end_line: 1,
                        total_lines: 1,
                        content_hash: hash,
                    }
                })
                .ok_or_else(|| anyhow::anyhow!(std::io::Error::from(std::io::ErrorKind::NotFound)))
        }
    }

    impl EnvironmentService for TestServices {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            let mut env: Environment = Faker.fake();
            if let Some(cwd) = &self.cwd {
                env.cwd = cwd.clone();
            } else {
                // Use a deterministic cwd that won't match any test paths
                env.cwd = PathBuf::from("/deterministic/test/cwd");
            }
            env
        }
    }

    #[async_trait::async_trait]
    impl ContextEngineService for TestServices {
        async fn sync_codebase(
            &self,
            _path: PathBuf,
            _batch_size: usize,
        ) -> anyhow::Result<forge_domain::FileUploadResponse> {
            use forge_domain::{FileUploadInfo, WorkspaceId};
            self.sync_called.store(true, Ordering::SeqCst);
            Ok(forge_domain::FileUploadResponse::new(
                WorkspaceId::generate(),
                0,
                FileUploadInfo::default(),
            ))
        }

        async fn query_codebase(
            &self,
            _path: PathBuf,
            _params: forge_domain::SearchParams<'_>,
        ) -> anyhow::Result<Vec<forge_domain::CodeSearchResult>> {
            Ok(vec![])
        }

        async fn list_codebase(&self) -> anyhow::Result<Vec<forge_domain::WorkspaceInfo>> {
            Ok(vec![])
        }

        async fn get_workspace_info(
            &self,
            _path: PathBuf,
        ) -> anyhow::Result<Option<forge_domain::WorkspaceInfo>> {
            Ok(None)
        }

        async fn delete_codebase(
            &self,
            _workspace_id: &forge_domain::WorkspaceId,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn is_indexed(&self, _path: &std::path::Path) -> anyhow::Result<bool> {
            Ok(self.indexed)
        }

        async fn is_authenticated(&self) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn create_auth_credentials(&self) -> anyhow::Result<forge_domain::WorkspaceAuth> {
            use forge_domain::UserId;
            Ok(forge_domain::WorkspaceAuth::new(
                UserId::generate(),
                "test-key".to_string().into(),
            ))
        }
    }

    fn fixture(
        files: HashMap<String, String>,
        tracked_files: HashMap<String, Option<String>>,
    ) -> (ChangedFiles<TestServices>, Conversation, Arc<TestServices>) {
        fixture_with_options(files, tracked_files, None, false)
    }

    fn fixture_with_options(
        files: HashMap<String, String>,
        tracked_files: HashMap<String, Option<String>>,
        cwd: Option<PathBuf>,
        indexed: bool,
    ) -> (ChangedFiles<TestServices>, Conversation, Arc<TestServices>) {
        let services = Arc::new(TestServices { files, cwd, indexed, ..Default::default() });
        let agent = Agent::new(
            AgentId::new("test"),
            ProviderId::ANTHROPIC,
            ModelId::new("test-model"),
        );
        let changed_files = ChangedFiles::new(services.clone(), agent);

        let mut metrics = Metrics::default();
        for (path, hash) in tracked_files {
            metrics
                .file_operations
                .insert(path, FileOperation::new(ToolKind::Write).content_hash(hash));
        }

        let conversation = Conversation::new(ConversationId::generate()).metrics(metrics);

        (changed_files, conversation, services)
    }

    #[tokio::test]
    async fn test_no_changes_detected() {
        let content = "hello world";
        let hash = crate::compute_hash(content);

        let (service, mut conversation, _) = fixture(
            [("/test/file.txt".into(), content.into())].into(),
            [("/test/file.txt".into(), Some(hash))].into(),
        );

        conversation.context = Some(Context::default().add_message(ContextMessage::user(
            "Hey, there!",
            Some(ModelId::new("test")),
        )));

        let actual = service.handle_external_changes(conversation.clone()).await;

        assert_eq!(actual.context.clone().unwrap_or_default().messages.len(), 1);
        assert_eq!(actual.context, conversation.context);
    }

    #[tokio::test]
    async fn test_changes_detected_adds_notification() {
        let old_hash = crate::compute_hash("old content");
        let new_content = "new content";

        let (service, conversation, _) = fixture(
            [("/test/file.txt".into(), new_content.into())].into(),
            [("/test/file.txt".into(), Some(old_hash))].into(),
        );

        let actual = service.handle_external_changes(conversation).await;

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

        let (service, conversation, _) = fixture(
            [("/test/file.txt".into(), new_content.into())].into(),
            [("/test/file.txt".into(), Some(old_hash))].into(),
        );

        let actual = service.handle_external_changes(conversation).await;

        let updated_hash = actual
            .metrics
            .file_operations
            .get("/test/file.txt")
            .and_then(|m| m.content_hash.clone());

        assert_eq!(updated_hash, Some(new_hash));
    }

    #[tokio::test]
    async fn test_multiple_files_changed() {
        let (service, conversation, _) = fixture(
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

        let actual = service.handle_external_changes(conversation).await;

        let message = actual.context.unwrap().messages[0]
            .content()
            .unwrap()
            .to_string();

        insta::assert_snapshot!(message);
    }

    #[tokio::test]
    async fn test_uses_relative_paths_within_cwd() {
        let old_hash = crate::compute_hash("old content");
        let new_content = "new content";
        let cwd = PathBuf::from("/home/user/project");
        let absolute_path = "/home/user/project/src/main.rs";

        let (service, conversation, _) = fixture_with_options(
            [(absolute_path.into(), new_content.into())].into(),
            [(absolute_path.into(), Some(old_hash))].into(),
            Some(cwd),
            false,
        );

        let actual = service.handle_external_changes(conversation).await;

        let message = actual.context.unwrap().messages[0]
            .content()
            .unwrap()
            .to_string();

        let expected = "<information>\n<critical>The following files have been modified externally. Please re-read them if its relevant for the task.</critical>\n<files>\n<file>src/main.rs</file>\n</files>\n</information>";

        assert_eq!(message, expected);
    }

    #[tokio::test(start_paused = true)]
    async fn test_triggers_reindex_when_indexed() {
        let old_hash = crate::compute_hash("old content");
        let (service, conversation, services) = fixture_with_options(
            [("/test/file.txt".into(), "new content".into())].into(),
            [("/test/file.txt".into(), Some(old_hash))].into(),
            None,
            true,
        );

        service.handle_external_changes(conversation).await;
        tokio::time::advance(std::time::Duration::from_millis(1)).await;

        assert!(services.sync_called.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn test_skips_reindex_when_not_indexed() {
        let old_hash = crate::compute_hash("old content");
        let (service, conversation, services) = fixture_with_options(
            [("/test/file.txt".into(), "new content".into())].into(),
            [("/test/file.txt".into(), Some(old_hash))].into(),
            None,
            false,
        );

        service.handle_external_changes(conversation).await;
        tokio::time::advance(std::time::Duration::from_millis(1)).await;

        assert!(!services.sync_called.load(Ordering::SeqCst));
    }
}
