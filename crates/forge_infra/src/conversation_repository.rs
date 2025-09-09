use anyhow::Result;
use async_trait::async_trait;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use forge_app::domain::Conversation;
use forge_services::ConversationStorageInfra;

use crate::models::{NewConversationRecord, UpsertConversationRecord};
use crate::schema::conversations;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// SQLite-based implementation of ConversationStorageInfra
pub struct ConversationRepository {
    database_path: std::path::PathBuf,
}

impl ConversationRepository {
    /// Create and initialize the conversation repository with migrations
    pub fn init(database_path: std::path::PathBuf) -> Result<Self> {
        // Ensure the parent directory exists
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let database_url = database_path.to_string_lossy().to_string();
        let mut connection = SqliteConnection::establish(&database_url)?;

        // Run migrations
        connection
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Failed to run database migrations: {}", e))?;

        Ok(Self { database_path })
    }

    pub fn get_connection(&self) -> Result<SqliteConnection> {
        let database_url = self.database_path.to_string_lossy().to_string();
        let connection = SqliteConnection::establish(&database_url)?;
        Ok(connection)
    }
}

#[async_trait]
impl ConversationStorageInfra for ConversationRepository {
    async fn save(&self, conversation: &Conversation) -> Result<()> {
        let mut connection = self.get_connection()?;
        let new_record =
            NewConversationRecord::try_from((conversation, conversation.workspace_id.clone()))?;

        diesel::insert_into(conversations::table)
            .values(&new_record)
            .execute(&mut connection)?;
        Ok(())
    }

    async fn find_by_id(&self, conversation_id: &str) -> Result<Option<Conversation>> {
        let mut connection = self.get_connection()?;

        let record = conversations::table
            .filter(conversations::conversation_id.eq(conversation_id))
            .first(&mut connection)
            .optional()?;

        if let Some(record) = record {
            let conversation = Conversation::try_from(&record)?;
            Ok(Some(conversation))
        } else {
            Ok(None)
        }
    }

    async fn find_by_workspace_id(&self, workspace_id: &str) -> Result<Vec<Conversation>> {
        let mut connection = self.get_connection()?;

        let records: Vec<crate::models::ConversationRecord> = conversations::table
            .filter(conversations::workspace_id.eq(workspace_id))
            .order(conversations::updated_at.desc())
            .load(&mut connection)?;

        Ok(records
            .into_iter()
            .filter_map(|record| Conversation::try_from(&record).ok())
            .collect())
    }

    async fn upsert(&self, conversation: &Conversation) -> Result<()> {
        let mut connection = self.get_connection()?;
        let upsert_record =
            UpsertConversationRecord::try_from((conversation, conversation.workspace_id.clone()))?;

        diesel::insert_into(conversations::table)
            .values(&upsert_record)
            .on_conflict(conversations::conversation_id)
            .do_update()
            .set((
                conversations::context.eq(upsert_record.context.clone()),
                conversations::updated_at.eq(upsert_record.updated_at),
            ))
            .execute(&mut connection)?;
        Ok(())
    }

    async fn find_latest_by_workspace_id(
        &self,
        workspace_id: &str,
    ) -> Result<Option<Conversation>> {
        let mut connection = self.get_connection()?;

        let record: Option<crate::models::ConversationRecord> = conversations::table
            .filter(conversations::workspace_id.eq(workspace_id))
            .order(conversations::updated_at.desc())
            .first(&mut connection)
            .optional()?;

        if let Some(record) = record {
            let conversation = Conversation::try_from(&record)?;
            Ok(Some(conversation))
        } else {
            Ok(None)
        }
    }
}
