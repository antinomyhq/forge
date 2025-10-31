use std::hash::Hash;
use std::path::Path;

use anyhow::Result;
use serde::de::DeserializeOwned;

use crate::{AppConfig, Conversation, ConversationId, Provider, ProviderId, Snapshot};

/// Repository for managing file snapshots
///
/// This repository provides operations for creating and restoring file
/// snapshots, enabling undo functionality for file modifications.
#[async_trait::async_trait]
pub trait SnapshotRepository: Send + Sync {
    /// Inserts a new snapshot for the given file path
    ///
    /// # Arguments
    /// * `file_path` - Path to the file to snapshot
    ///
    /// # Errors
    /// Returns an error if the snapshot creation fails
    async fn insert_snapshot(&self, file_path: &Path) -> Result<Snapshot>;

    /// Restores the most recent snapshot for the given file path
    ///
    /// # Arguments
    /// * `file_path` - Path to the file to restore
    ///
    /// # Errors
    /// Returns an error if no snapshot exists or restoration fails
    async fn undo_snapshot(&self, file_path: &Path) -> Result<()>;
}

/// Generic cache repository for content-addressable storage.
///
/// This trait provides an abstraction over caching operations with support for
/// arbitrary key and value types. Keys must be hashable and serializable, while
/// values must be serializable. The trait is designed to work with
/// content-addressable storage systems like cacache.
///
/// All operations return `anyhow::Result` for consistent error handling across
/// the infrastructure layer.
#[async_trait::async_trait]
pub trait KVStore: Send + Sync {
    /// Retrieves a value from the cache by its key.
    ///
    /// # Arguments
    /// * `key` - The key to look up in the cache
    ///
    /// # Errors
    /// Returns an error if the cache operation fails
    async fn cache_get<K, V>(&self, key: &K) -> Result<Option<V>>
    where
        K: Hash + Sync,
        V: serde::Serialize + DeserializeOwned + Send;

    /// Stores a value in the cache with the given key.
    ///
    /// If the key already exists, the value is overwritten.
    /// Uses content-addressable storage for integrity verification.
    ///
    /// # Arguments
    /// * `key` - The key to store the value under
    /// * `value` - The value to cache
    ///
    /// # Errors
    /// Returns an error if the cache operation fails
    async fn cache_set<K, V>(&self, key: &K, value: &V) -> Result<()>
    where
        K: Hash + Sync,
        V: serde::Serialize + Sync;

    /// Clears all entries from the cache.
    ///
    /// This operation removes all cached data. Use with caution.
    ///
    /// # Errors
    /// Returns an error if the cache clear operation fails
    async fn cache_clear(&self) -> Result<()>;
}

/// Repository for managing conversation persistence
///
/// This repository provides CRUD operations for conversations, including
/// creating, retrieving, and listing conversations.
#[async_trait::async_trait]
pub trait ConversationRepository: Send + Sync {
    /// Creates or updates a conversation
    ///
    /// # Arguments
    /// * `conversation` - The conversation to persist
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn upsert_conversation(&self, conversation: Conversation) -> Result<()>;

    /// Retrieves a conversation by its ID
    ///
    /// # Arguments
    /// * `conversation_id` - The ID of the conversation to retrieve
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn get_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<Conversation>>;

    /// Retrieves all conversations with an optional limit
    ///
    /// # Arguments
    /// * `limit` - Optional maximum number of conversations to retrieve
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn get_all_conversations(
        &self,
        limit: Option<usize>,
    ) -> Result<Option<Vec<Conversation>>>;

    /// Retrieves the most recent conversation
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn get_last_conversation(&self) -> Result<Option<Conversation>>;
}

#[async_trait::async_trait]
pub trait ProviderRepository: Send + Sync {
    async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>>;
    async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider>;
}

#[async_trait::async_trait]
pub trait AppConfigRepository: Send + Sync {
    async fn get_app_config(&self) -> anyhow::Result<AppConfig>;
    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()>;
}
