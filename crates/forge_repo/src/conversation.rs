use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, NaiveDateTime, Utc};
use derive_more::From;
use diesel::prelude::*;
use forge_domain::{
    Context, Conversation, ConversationId, ConversationRepository, FileOperation, MetaData,
    Metrics, ToolKind, WorkspaceHash,
};
use serde::{Deserialize, Serialize};

use crate::database::schema::conversations;
use crate::database::DatabasePool;

/// Database representation of file change metrics
/// Mirrors `forge_domain::FileChangeMetrics` for compile-time safety
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileChangeMetricsRecord {
    lines_added: u64,
    lines_removed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<ToolKind>,
}

impl From<&FileOperation> for FileChangeMetricsRecord {
    fn from(metrics: &FileOperation) -> Self {
        Self {
            lines_added: metrics.lines_added,
            lines_removed: metrics.lines_removed,
            content_hash: metrics.content_hash.clone(),
            tool: Some(metrics.tool),
        }
    }
}

impl From<FileChangeMetricsRecord> for FileOperation {
    fn from(record: FileChangeMetricsRecord) -> Self {
        // Use Write as default tool for old records without tool field
        let tool = record.tool.unwrap_or(ToolKind::Write);
        Self::new(tool)
            .lines_added(record.lines_added)
            .lines_removed(record.lines_removed)
            .content_hash(record.content_hash)
    }
}

/// Represents either a single file operation or array (for backward
/// compatibility)
#[derive(Debug, Clone, Serialize, Deserialize, From)]
#[serde(untagged)]
enum FileOperationOrArray {
    Single(FileChangeMetricsRecord),
    Array(Vec<FileChangeMetricsRecord>),
}

/// Database representation of session metrics
/// Mirrors `forge_domain::Metrics` for compile-time safety
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetricsRecord {
    started_at: Option<DateTime<Utc>>,
    files_changed: HashMap<String, FileOperationOrArray>,
}

impl From<&Metrics> for MetricsRecord {
    fn from(metrics: &Metrics) -> Self {
        Self {
            started_at: metrics.started_at,
            files_changed: metrics
                .file_operations
                .iter()
                .map(|(path, file_metrics)| {
                    (
                        path.clone(),
                        FileOperationOrArray::Single(file_metrics.into()),
                    )
                })
                .collect(),
        }
    }
}

impl From<MetricsRecord> for Metrics {
    fn from(record: MetricsRecord) -> Self {
        Self {
            started_at: record.started_at,
            file_operations: record
                .files_changed
                .into_iter()
                .filter_map(|(path, file_record)| {
                    let operation = match file_record {
                        // If it's an array, take the last operation (most recent)
                        FileOperationOrArray::Array(mut arr) if !arr.is_empty() => {
                            arr.pop().unwrap().into()
                        }
                        // If it's a single object, use it directly
                        FileOperationOrArray::Single(record) => record.into(),
                        // If it's an empty array, skip this file
                        FileOperationOrArray::Array(_) => return None,
                    };
                    Some((path, operation))
                })
                .collect(),
        }
    }
}

// Database model for conversations table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = conversations)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct ConversationRecord {
    conversation_id: String,
    title: Option<String>,
    workspace_id: i64,
    context: Option<String>,
    created_at: NaiveDateTime,
    updated_at: Option<NaiveDateTime>,
    metrics: Option<String>,
}

impl ConversationRecord {
    fn new(conversation: Conversation, workspace_id: WorkspaceHash) -> Self {
        let context = conversation
            .context
            .as_ref()
            .filter(|ctx| !ctx.messages.is_empty())
            .and_then(|ctx| serde_json::to_string(ctx).ok());
        let updated_at = context.as_ref().map(|_| Utc::now().naive_utc());
        let metrics_record = MetricsRecord::from(&conversation.metrics);
        let metrics = serde_json::to_string(&metrics_record).ok();

        Self {
            conversation_id: conversation.id.into_string(),
            title: conversation.title.clone(),
            context,
            created_at: conversation.metadata.created_at.naive_utc(),
            updated_at,
            workspace_id: workspace_id.id() as i64,
            metrics,
        }
    }
}

impl TryFrom<ConversationRecord> for Conversation {
    type Error = anyhow::Error;
    fn try_from(record: ConversationRecord) -> anyhow::Result<Self> {
        let id = ConversationId::parse(record.conversation_id)?;
        let context = record
            .context
            .and_then(|ctx| serde_json::from_str::<Context>(&ctx).ok());

        // Deserialize metrics using MetricsRecord for compile-time safety
        let metrics = record
            .metrics
            .and_then(|m| serde_json::from_str::<MetricsRecord>(&m).ok())
            .map(Metrics::from)
            .unwrap_or_else(|| Metrics::default().started_at(record.created_at.and_utc()));

        Ok(Conversation::new(id)
            .context(context)
            .title(record.title)
            .metrics(metrics)
            .metadata(
                MetaData::new(record.created_at.and_utc())
                    .updated_at(record.updated_at.map(|updated_at| updated_at.and_utc())),
            ))
    }
}

pub struct ConversationRepositoryImpl {
    pool: Arc<DatabasePool>,
    wid: WorkspaceHash,
}

impl ConversationRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>, workspace_id: WorkspaceHash) -> Self {
        Self { pool, wid: workspace_id }
    }
}

#[async_trait::async_trait]
impl ConversationRepository for ConversationRepositoryImpl {
    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;

        let wid = self.wid;
        let record = ConversationRecord::new(conversation, wid);
        diesel::insert_into(conversations::table)
            .values(&record)
            .on_conflict(conversations::conversation_id)
            .do_update()
            .set((
                conversations::title.eq(&record.title),
                conversations::context.eq(&record.context),
                conversations::updated_at.eq(record.updated_at),
                conversations::metrics.eq(&record.metrics),
            ))
            .execute(&mut connection)?;
        Ok(())
    }

    async fn get_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        let mut connection = self.pool.get_connection()?;

        let record: Option<ConversationRecord> = conversations::table
            .filter(conversations::conversation_id.eq(conversation_id.into_string()))
            .first(&mut connection)
            .optional()?;

        match record {
            Some(record) => Ok(Some(Conversation::try_from(record)?)),
            None => Ok(None),
        }
    }

    async fn get_all_conversations(
        &self,
        limit: Option<usize>,
    ) -> anyhow::Result<Option<Vec<Conversation>>> {
        let mut connection = self.pool.get_connection()?;

        let workspace_id = self.wid.id() as i64;
        let mut query = conversations::table
            .filter(conversations::workspace_id.eq(&workspace_id))
            .filter(conversations::context.is_not_null())
            .order(conversations::updated_at.desc())
            .into_boxed();

        if let Some(limit_value) = limit {
            query = query.limit(limit_value as i64);
        }

        let records: Vec<ConversationRecord> = query.load(&mut connection)?;

        if records.is_empty() {
            return Ok(None);
        }

        let conversations: Result<Vec<Conversation>, _> =
            records.into_iter().map(Conversation::try_from).collect();
        Ok(Some(conversations?))
    }

    async fn get_last_conversation(&self) -> anyhow::Result<Option<Conversation>> {
        let mut connection = self.pool.get_connection()?;
        let workspace_id = self.wid.id() as i64;
        let record: Option<ConversationRecord> = conversations::table
            .filter(conversations::workspace_id.eq(&workspace_id))
            .filter(conversations::context.is_not_null())
            .order(conversations::updated_at.desc())
            .first(&mut connection)
            .optional()?;
        let conversation = match record {
            Some(record) => Some(Conversation::try_from(record)?),
            None => None,
        };
        Ok(conversation)
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::ContextMessage;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::database::DatabasePool;

    fn repository() -> anyhow::Result<ConversationRepositoryImpl> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        Ok(ConversationRepositoryImpl::new(pool, WorkspaceHash::new(0)))
    }

    #[tokio::test]
    async fn test_upsert_and_find_by_id() -> anyhow::Result<()> {
        let fixture = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(fixture.clone()).await?;

        let actual = repo.get_conversation(&fixture.id).await?;
        assert!(actual.is_some());
        let retrieved = actual.unwrap();
        assert_eq!(retrieved.id, fixture.id);
        assert_eq!(retrieved.title, fixture.title);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_by_id_non_existing() -> anyhow::Result<()> {
        let repo = repository()?;
        let non_existing_id = ConversationId::generate();

        let actual = repo.get_conversation(&non_existing_id).await?;

        assert!(actual.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_updates_existing_conversation() -> anyhow::Result<()> {
        let mut fixture = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        // Insert initial conversation
        repo.upsert_conversation(fixture.clone()).await?;

        // Update the conversation
        fixture = fixture.title(Some("Updated Title".to_string()));
        repo.upsert_conversation(fixture.clone()).await?;

        let actual = repo.get_conversation(&fixture.id).await?;
        assert!(actual.is_some());
        assert_eq!(actual.unwrap().title, Some("Updated Title".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn test_find_all_conversations() -> anyhow::Result<()> {
        let context1 = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let context2 = Context::default().messages(vec![ContextMessage::user("World", None)]);
        let conversation1 = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()))
            .context(Some(context1));
        let conversation2 = Conversation::new(ConversationId::generate())
            .title(Some("Second Conversation".to_string()))
            .context(Some(context2));
        let repo = repository()?;

        repo.upsert_conversation(conversation1.clone()).await?;
        repo.upsert_conversation(conversation2.clone()).await?;

        let actual = repo.get_all_conversations(None).await?;

        assert!(actual.is_some());
        let conversations = actual.unwrap();
        assert_eq!(conversations.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_all_conversations_with_limit() -> anyhow::Result<()> {
        let context1 = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let context2 = Context::default().messages(vec![ContextMessage::user("World", None)]);
        let conversation1 = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()))
            .context(Some(context1));
        let conversation2 = Conversation::new(ConversationId::generate()).context(Some(context2));
        let repo = repository()?;

        repo.upsert_conversation(conversation1).await?;
        repo.upsert_conversation(conversation2).await?;

        let actual = repo.get_all_conversations(Some(1)).await?;

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_all_conversations_empty() -> anyhow::Result<()> {
        let repo = repository()?;

        let actual = repo.get_all_conversations(None).await?;

        assert!(actual.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_with_context() -> anyhow::Result<()> {
        let context = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let conversation_with_context = Conversation::new(ConversationId::generate())
            .title(Some("Conversation with Context".to_string()))
            .context(Some(context));
        let conversation_without_context = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;
        repo.upsert_conversation(conversation_with_context.clone())
            .await?;

        let actual = repo.get_last_conversation().await?;

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().id, conversation_with_context.id);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_no_context() -> anyhow::Result<()> {
        let conversation_without_context = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;

        let actual = repo.get_last_conversation().await?;

        assert!(actual.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_ignores_empty_context() -> anyhow::Result<()> {
        let conversation_with_empty_context = Conversation::new(ConversationId::generate())
            .title(Some("Conversation with Empty Context".to_string()))
            .context(Some(Context::default()));
        let conversation_without_context = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;
        repo.upsert_conversation(conversation_with_empty_context)
            .await?;

        let actual = repo.get_last_conversation().await?;

        assert!(actual.is_none()); // Should not find conversations with empty contexts
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation() -> anyhow::Result<()> {
        let fixture = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));

        let actual = ConversationRecord::new(fixture.clone(), WorkspaceHash::new(0));

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(actual.title, Some("Test Conversation".to_string()));

        assert_eq!(actual.context, None);
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation_with_context() -> anyhow::Result<()> {
        let context = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let fixture = Conversation::new(ConversationId::generate())
            .title(Some("Conversation with Context".to_string()))
            .context(Some(context));

        let actual = ConversationRecord::new(fixture.clone(), WorkspaceHash::new(0));

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(actual.title, Some("Conversation with Context".to_string()));

        assert!(actual.context.is_some());
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation_with_empty_context() -> anyhow::Result<()> {
        let fixture = Conversation::new(ConversationId::generate())
            .title(Some("Conversation with Empty Context".to_string()))
            .context(Some(Context::default()));

        let actual = ConversationRecord::new(fixture.clone(), WorkspaceHash::new(0));

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(
            actual.title,
            Some("Conversation with Empty Context".to_string())
        );

        assert!(actual.context.is_none()); // Empty context should be filtered out
        Ok(())
    }

    #[test]
    fn test_conversation_from_conversation_record() -> anyhow::Result<()> {
        let test_id = ConversationId::generate();
        let fixture = ConversationRecord {
            conversation_id: test_id.into_string(),
            title: Some("Test Conversation".to_string()),
            context: None,
            created_at: Utc::now().naive_utc(),
            updated_at: None,
            workspace_id: 0,
            metrics: None,
        };

        let actual = Conversation::try_from(fixture)?;

        assert_eq!(actual.id, test_id);
        assert_eq!(actual.title, Some("Test Conversation".to_string()));
        assert_eq!(actual.context, None);
        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_and_retrieve_conversation_with_metrics() -> anyhow::Result<()> {
        let repo = repository()?;

        // Create a conversation with metrics
        let metrics = Metrics::default()
            .started_at(Utc::now())
            .insert(
                "src/main.rs".to_string(),
                FileOperation::new(ToolKind::Write)
                    .lines_added(10u64)
                    .lines_removed(5u64)
                    .content_hash(Some("abc123def456".to_string())),
            )
            .insert(
                "src/lib.rs".to_string(),
                FileOperation::new(ToolKind::Write)
                    .lines_added(3u64)
                    .lines_removed(2u64)
                    .content_hash(Some("789xyz456abc".to_string())),
            );

        let fixture = Conversation::generate().metrics(metrics.clone());

        // Save the conversation
        repo.upsert_conversation(fixture.clone()).await?;

        // Retrieve the conversation
        let actual = repo
            .get_conversation(&fixture.id)
            .await?
            .expect("Conversation should exist");

        // Verify metrics are preserved
        assert_eq!(actual.metrics.file_operations.len(), 2);
        let main_metrics = actual.metrics.file_operations.get("src/main.rs").unwrap();
        assert_eq!(main_metrics.lines_added, 10);
        assert_eq!(main_metrics.lines_removed, 5);
        assert_eq!(main_metrics.content_hash, Some("abc123def456".to_string()));

        let lib_metrics = actual.metrics.file_operations.get("src/lib.rs").unwrap();
        assert_eq!(lib_metrics.lines_added, 3);
        assert_eq!(lib_metrics.lines_removed, 2);
        assert_eq!(lib_metrics.content_hash, Some("789xyz456abc".to_string()));

        Ok(())
    }

    #[test]
    fn test_metrics_record_conversion_preserves_all_fields() {
        // This test ensures compile-time safety: if Metrics schema changes,
        // this test will fail to compile, alerting us to update MetricsRecord
        let fixture = Metrics::default().started_at(Utc::now()).insert(
            "test.rs".to_string(),
            FileOperation::new(ToolKind::Write)
                .lines_added(5u64)
                .lines_removed(3u64)
                .content_hash(Some("test_hash_123".to_string())),
        );

        // Convert to record and back
        let record = MetricsRecord::from(&fixture);
        let actual = Metrics::from(record);

        // Verify all fields are preserved
        assert_eq!(actual.started_at, fixture.started_at);
        assert_eq!(actual.file_operations.len(), fixture.file_operations.len());

        let actual_file = actual.file_operations.get("test.rs").unwrap();
        let expected_file = fixture.file_operations.get("test.rs").unwrap();
        assert_eq!(actual_file.lines_added, expected_file.lines_added);
        assert_eq!(actual_file.lines_removed, expected_file.lines_removed);
        assert_eq!(actual_file.content_hash, expected_file.content_hash);
    }

    #[test]
    fn test_deserialize_old_format_without_tool_field() {
        // Old format from database: missing tool and content_hash fields
        let json = r#"{
            "started_at": "2024-01-01T00:00:00Z",
            "files_changed": {
                "src/main.rs": {
                    "lines_added": 10,
                    "lines_removed": 5
                },
                "src/lib.rs": {
                    "lines_added": 3,
                    "lines_removed": 2
                }
            }
        }"#;

        let record: MetricsRecord = serde_json::from_str(json).unwrap();
        let actual = Metrics::from(record);

        // Verify files are loaded
        assert_eq!(actual.file_operations.len(), 2);

        // Verify main.rs
        let main_file = actual.file_operations.get("src/main.rs").unwrap();
        assert_eq!(main_file.lines_added, 10);
        assert_eq!(main_file.lines_removed, 5);
        assert_eq!(main_file.content_hash, None);
        assert_eq!(main_file.tool, ToolKind::Write); // Default tool

        // Verify lib.rs
        let lib_file = actual.file_operations.get("src/lib.rs").unwrap();
        assert_eq!(lib_file.lines_added, 3);
        assert_eq!(lib_file.lines_removed, 2);
        assert_eq!(lib_file.content_hash, None);
        assert_eq!(lib_file.tool, ToolKind::Write); // Default tool
    }

    #[test]
    fn test_deserialize_array_format_takes_last_operation() {
        // Array format from database: multiple operations per file
        let json = r#"{
            "started_at": "2024-01-01T00:00:00Z",
            "files_changed": {
                "src/main.rs": [
                    {
                        "lines_added": 2,
                        "lines_removed": 4,
                        "content_hash": "hash1",
                        "tool": "read"
                    },
                    {
                        "lines_added": 1,
                        "lines_removed": 1,
                        "content_hash": "hash2",
                        "tool": "patch"
                    },
                    {
                        "lines_added": 5,
                        "lines_removed": 3,
                        "content_hash": "hash3",
                        "tool": "write"
                    }
                ]
            }
        }"#;

        let record: MetricsRecord = serde_json::from_str(json).unwrap();
        let actual = Metrics::from(record);

        // Verify only the last operation is kept
        assert_eq!(actual.file_operations.len(), 1);

        let main_file = actual.file_operations.get("src/main.rs").unwrap();
        assert_eq!(main_file.lines_added, 5);
        assert_eq!(main_file.lines_removed, 3);
        assert_eq!(main_file.content_hash, Some("hash3".to_string()));
        assert_eq!(main_file.tool, ToolKind::Write);
    }

    #[test]
    fn test_deserialize_array_format_with_empty_array() {
        // Array format with empty array should be skipped
        let json = r#"{
            "started_at": "2024-01-01T00:00:00Z",
            "files_changed": {
                "src/main.rs": [],
                "src/lib.rs": {
                    "lines_added": 5,
                    "lines_removed": 2,
                    "content_hash": "hash1",
                    "tool": "patch"
                }
            }
        }"#;

        let record: MetricsRecord = serde_json::from_str(json).unwrap();
        let actual = Metrics::from(record);

        // Empty array should be skipped, only lib.rs should be present
        assert_eq!(actual.file_operations.len(), 1);
        assert!(actual.file_operations.contains_key("src/lib.rs"));
        assert!(!actual.file_operations.contains_key("src/main.rs"));
    }

    #[test]
    fn test_deserialize_current_format_with_all_fields() {
        // Current format: single object with all fields
        let json = r#"{
            "started_at": "2024-01-01T00:00:00Z",
            "files_changed": {
                "src/main.rs": {
                    "lines_added": 10,
                    "lines_removed": 5,
                    "content_hash": "abc123def456",
                    "tool": "patch"
                },
                "src/lib.rs": {
                    "lines_added": 3,
                    "lines_removed": 2,
                    "content_hash": "789xyz456abc",
                    "tool": "write"
                }
            }
        }"#;

        let record: MetricsRecord = serde_json::from_str(json).unwrap();
        let actual = Metrics::from(record);

        // Verify all fields are preserved
        assert_eq!(actual.file_operations.len(), 2);

        let main_file = actual.file_operations.get("src/main.rs").unwrap();
        assert_eq!(main_file.lines_added, 10);
        assert_eq!(main_file.lines_removed, 5);
        assert_eq!(main_file.content_hash, Some("abc123def456".to_string()));
        assert_eq!(main_file.tool, ToolKind::Patch);

        let lib_file = actual.file_operations.get("src/lib.rs").unwrap();
        assert_eq!(lib_file.lines_added, 3);
        assert_eq!(lib_file.lines_removed, 2);
        assert_eq!(lib_file.content_hash, Some("789xyz456abc".to_string()));
        assert_eq!(lib_file.tool, ToolKind::Write);
    }

    #[test]
    fn test_deserialize_mixed_format() {
        // Mix of old format, array format, and current format
        let json = r#"{
            "started_at": "2024-01-01T00:00:00Z",
            "files_changed": {
                "old_file.rs": {
                    "lines_added": 10,
                    "lines_removed": 5
                },
                "array_file.rs": [
                    {
                        "lines_added": 1,
                        "lines_removed": 2,
                        "content_hash": "hash1",
                        "tool": "read"
                    },
                    {
                        "lines_added": 3,
                        "lines_removed": 4,
                        "content_hash": "hash2",
                        "tool": "patch"
                    }
                ],
                "current_file.rs": {
                    "lines_added": 7,
                    "lines_removed": 8,
                    "content_hash": "hash3",
                    "tool": "write"
                }
            }
        }"#;

        let record: MetricsRecord = serde_json::from_str(json).unwrap();
        let actual = Metrics::from(record);

        assert_eq!(actual.file_operations.len(), 3);

        // Old format file
        let old_file = actual.file_operations.get("old_file.rs").unwrap();
        assert_eq!(old_file.lines_added, 10);
        assert_eq!(old_file.lines_removed, 5);
        assert_eq!(old_file.content_hash, None);
        assert_eq!(old_file.tool, ToolKind::Write); // Default

        // Array format file (should have last operation)
        let array_file = actual.file_operations.get("array_file.rs").unwrap();
        assert_eq!(array_file.lines_added, 3);
        assert_eq!(array_file.lines_removed, 4);
        assert_eq!(array_file.content_hash, Some("hash2".to_string()));
        assert_eq!(array_file.tool, ToolKind::Patch);

        // Current format file
        let current_file = actual.file_operations.get("current_file.rs").unwrap();
        assert_eq!(current_file.lines_added, 7);
        assert_eq!(current_file.lines_removed, 8);
        assert_eq!(current_file.content_hash, Some("hash3".to_string()));
        assert_eq!(current_file.tool, ToolKind::Write);
    }

    #[test]
    fn test_serialize_current_format() {
        // Test that we always serialize in the current format (single object)
        let fixture = Metrics::default().started_at(Utc::now()).insert(
            "src/main.rs".to_string(),
            FileOperation::new(ToolKind::Patch)
                .lines_added(10u64)
                .lines_removed(5u64)
                .content_hash(Some("abc123".to_string())),
        );

        let record = MetricsRecord::from(&fixture);
        let json = serde_json::to_string(&record).unwrap();

        // Verify it's not an array format
        assert!(!json.contains("[{"));
        // Verify it contains the tool field
        assert!(json.contains("\"tool\":\"patch\""));
        // Verify structure is correct
        assert!(json.contains("\"lines_added\":10"));
        assert!(json.contains("\"lines_removed\":5"));
        assert!(json.contains("\"content_hash\":\"abc123\""));
    }
}
