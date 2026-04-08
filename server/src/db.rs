use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

/// Hashes an API key using SHA-256 for secure storage.
fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Row returned from workspace queries.
pub struct WorkspaceRow {
    pub workspace_id: String,
    /// Owner user_id — used for ownership verification.
    #[allow(dead_code)]
    pub user_id: String,
    pub working_dir: String,
    pub min_chunk_size: u32,
    pub max_chunk_size: u32,
    pub created_at: String,
    pub node_count: u64,
}

/// SQLite-backed metadata storage for workspaces, API keys, and file references.
///
/// All public methods use `spawn_blocking` internally since rusqlite is synchronous.
/// The connection is wrapped in `Arc<Mutex<>>` for thread-safe access.
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens (or creates) the SQLite database and runs migrations.
    ///
    /// # Arguments
    /// * `path` - File path for the SQLite database
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite database at {path}"))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_keys (
                key TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%s', 'now'))
            );

            CREATE TABLE IF NOT EXISTS workspaces (
                workspace_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                working_dir TEXT NOT NULL,
                min_chunk_size INTEGER NOT NULL DEFAULT 100,
                max_chunk_size INTEGER NOT NULL DEFAULT 1500,
                created_at TEXT NOT NULL DEFAULT (strftime('%s', 'now')),
                UNIQUE(user_id, working_dir)
            );

            CREATE TABLE IF NOT EXISTS file_refs (
                workspace_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                node_id TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (strftime('%s', 'now')),
                PRIMARY KEY (workspace_id, file_path),
                FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id)
            );",
        )?;

        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Creates a new API key for a user.
    ///
    /// If `user_id` is `None`, generates a new UUID v4 user ID.
    /// Returns `(user_id, api_key)`.
    pub async fn create_api_key(&self, user_id: Option<&str>) -> Result<(String, String)> {
        let conn = self.conn.clone();
        let user_id = user_id.map(String::from);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let user_id = user_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let key = uuid::Uuid::new_v4().to_string();
            let key_hash = hash_api_key(&key);
            conn.execute(
                "INSERT INTO api_keys (key, user_id) VALUES (?1, ?2)",
                rusqlite::params![key_hash, user_id],
            )?;
            Ok((user_id, key)) // Return the raw key to the user, store hash
        })
        .await?
    }

    /// Validates an API key and returns the associated user ID.
    ///
    /// Returns `None` if the key is not found.
    pub async fn validate_api_key(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.clone();
        let key_hash = hash_api_key(key);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let mut stmt = conn.prepare("SELECT user_id FROM api_keys WHERE key = ?1")?;
            let result = stmt
                .query_row(rusqlite::params![key_hash], |row| row.get::<_, String>(0))
                .ok();
            Ok(result)
        })
        .await?
    }

    /// Creates a workspace or returns the existing one for the same `(user_id, working_dir)`.
    ///
    /// Returns `(workspace_id, working_dir, created_at, is_new)`.
    pub async fn create_workspace(
        &self,
        user_id: &str,
        working_dir: &str,
        min_chunk_size: u32,
        max_chunk_size: u32,
    ) -> Result<(String, String, String, bool)> {
        let conn = self.conn.clone();
        let user_id = user_id.to_string();
        let working_dir = working_dir.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;

            // Check if workspace already exists for this user + working_dir
            let mut stmt = conn.prepare(
                "SELECT workspace_id, working_dir, created_at FROM workspaces WHERE user_id = ?1 AND working_dir = ?2",
            )?;
            if let Ok((ws_id, wd, created)) = stmt.query_row(
                rusqlite::params![user_id, working_dir],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            ) {
                return Ok((ws_id, wd, created, false));
            }

            // Create new workspace — created_at via SQL DEFAULT (strftime('%s', 'now'))
            let workspace_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO workspaces (workspace_id, user_id, working_dir, min_chunk_size, max_chunk_size) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![workspace_id, user_id, working_dir, min_chunk_size, max_chunk_size],
            )?;

            // Read back created_at to return consistent value
            let created_at: String = conn.query_row(
                "SELECT created_at FROM workspaces WHERE workspace_id = ?1",
                rusqlite::params![workspace_id],
                |row| row.get(0),
            )?;

            Ok((workspace_id, working_dir, created_at, true))
        })
        .await?
    }

    /// Upserts a file reference (path + content hash) for a workspace.
    pub async fn upsert_file_ref(
        &self,
        workspace_id: &str,
        file_path: &str,
        file_hash: &str,
        node_id: &str,
    ) -> Result<()> {
        let conn = self.conn.clone();
        let workspace_id = workspace_id.to_string();
        let file_path = file_path.to_string();
        let file_hash = file_hash.to_string();
        let node_id = node_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            conn.execute(
                "INSERT INTO file_refs (workspace_id, file_path, file_hash, node_id)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(workspace_id, file_path) DO UPDATE SET
                    file_hash = excluded.file_hash,
                    node_id = excluded.node_id,
                    updated_at = strftime('%s', 'now')",
                rusqlite::params![workspace_id, file_path, file_hash, node_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Deletes file references for the given paths in a workspace.
    ///
    /// Uses a transaction to ensure atomicity — either all paths are deleted or none.
    /// Returns the number of deleted rows.
    pub async fn delete_file_refs(
        &self,
        workspace_id: &str,
        file_paths: &[String],
    ) -> Result<usize> {
        let conn = self.conn.clone();
        let workspace_id = workspace_id.to_string();
        let file_paths = file_paths.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let tx = conn.transaction()?;
            let mut deleted = 0usize;
            for path in &file_paths {
                deleted += tx.execute(
                    "DELETE FROM file_refs WHERE workspace_id = ?1 AND file_path = ?2",
                    rusqlite::params![workspace_id, path],
                )?;
            }
            tx.commit()?;
            Ok(deleted)
        })
        .await?
    }

    /// Lists all file references for a workspace.
    ///
    /// Returns `Vec<(node_id, file_path, file_hash)>`.
    pub async fn list_file_refs(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<(String, String, String)>> {
        let conn = self.conn.clone();
        let workspace_id = workspace_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let mut stmt = conn.prepare(
                "SELECT node_id, file_path, file_hash FROM file_refs WHERE workspace_id = ?1",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![workspace_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
    }

    /// Lists all workspaces belonging to a user, with file counts.
    pub async fn list_workspaces_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<WorkspaceRow>> {
        let conn = self.conn.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let mut stmt = conn.prepare(
                "SELECT w.workspace_id, w.user_id, w.working_dir, w.min_chunk_size, w.max_chunk_size, w.created_at,
                        COALESCE((SELECT COUNT(*) FROM file_refs f WHERE f.workspace_id = w.workspace_id), 0)
                 FROM workspaces w WHERE w.user_id = ?1",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![user_id], |row| {
                    Ok(WorkspaceRow {
                        workspace_id: row.get(0)?,
                        user_id: row.get(1)?,
                        working_dir: row.get(2)?,
                        min_chunk_size: row.get(3)?,
                        max_chunk_size: row.get(4)?,
                        created_at: row.get(5)?,
                        node_count: row.get(6)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
    }

    /// Retrieves a single workspace by ID.
    pub async fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRow>> {
        let conn = self.conn.clone();
        let workspace_id = workspace_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let mut stmt = conn.prepare(
                "SELECT w.workspace_id, w.user_id, w.working_dir, w.min_chunk_size, w.max_chunk_size, w.created_at,
                        COALESCE((SELECT COUNT(*) FROM file_refs f WHERE f.workspace_id = w.workspace_id), 0)
                 FROM workspaces w WHERE w.workspace_id = ?1",
            )?;
            let row = stmt
                .query_row(rusqlite::params![workspace_id], |row| {
                    Ok(WorkspaceRow {
                        workspace_id: row.get(0)?,
                        user_id: row.get(1)?,
                        working_dir: row.get(2)?,
                        min_chunk_size: row.get(3)?,
                        max_chunk_size: row.get(4)?,
                        created_at: row.get(5)?,
                        node_count: row.get(6)?,
                    })
                })
                .ok();
            Ok(row)
        })
        .await?
    }

    /// Deletes a workspace and all its file references in a single transaction.
    pub async fn delete_workspace(&self, workspace_id: &str) -> Result<()> {
        let conn = self.conn.clone();
        let workspace_id = workspace_id.to_string();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM file_refs WHERE workspace_id = ?1",
                rusqlite::params![workspace_id],
            )?;
            tx.execute(
                "DELETE FROM workspaces WHERE workspace_id = ?1",
                rusqlite::params![workspace_id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await?
    }

    /// Checks if a workspace belongs to the given user.
    pub async fn verify_workspace_owner(&self, workspace_id: &str, user_id: &str) -> Result<bool> {
        let conn = self.conn.clone();
        let workspace_id = workspace_id.to_string();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let mut stmt = conn.prepare(
                "SELECT 1 FROM workspaces WHERE workspace_id = ?1 AND user_id = ?2",
            )?;
            let exists = stmt
                .query_row(rusqlite::params![workspace_id, user_id], |_| Ok(()))
                .is_ok();
            Ok(exists)
        })
        .await?
    }
}
