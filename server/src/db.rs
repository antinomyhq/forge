use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Row returned from workspace queries.
pub struct WorkspaceRow {
    pub workspace_id: String,
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
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS workspaces (
                workspace_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                working_dir TEXT NOT NULL,
                min_chunk_size INTEGER NOT NULL DEFAULT 100,
                max_chunk_size INTEGER NOT NULL DEFAULT 1500,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(user_id, working_dir)
            );

            CREATE TABLE IF NOT EXISTS file_refs (
                workspace_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                node_id TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
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
            conn.execute(
                "INSERT INTO api_keys (key, user_id) VALUES (?1, ?2)",
                rusqlite::params![key, user_id],
            )?;
            Ok((user_id, key))
        })
        .await?
    }

    /// Validates an API key and returns the associated user ID.
    ///
    /// Returns `None` if the key is not found.
    pub async fn validate_api_key(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let mut stmt = conn.prepare("SELECT user_id FROM api_keys WHERE key = ?1")?;
            let result = stmt
                .query_row(rusqlite::params![key], |row| row.get::<_, String>(0))
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

            // Create new workspace
            let workspace_id = uuid::Uuid::new_v4().to_string();
            let created_at = chrono_now();
            conn.execute(
                "INSERT INTO workspaces (workspace_id, user_id, working_dir, min_chunk_size, max_chunk_size, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![workspace_id, user_id, working_dir, min_chunk_size, max_chunk_size, created_at],
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
                "INSERT INTO file_refs (workspace_id, file_path, file_hash, node_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(workspace_id, file_path) DO UPDATE SET
                    file_hash = excluded.file_hash,
                    node_id = excluded.node_id,
                    updated_at = excluded.updated_at",
                rusqlite::params![workspace_id, file_path, file_hash, node_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Deletes file references for the given paths in a workspace.
    ///
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
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            let mut deleted = 0usize;
            for path in &file_paths {
                deleted += conn.execute(
                    "DELETE FROM file_refs WHERE workspace_id = ?1 AND file_path = ?2",
                    rusqlite::params![workspace_id, path],
                )?;
            }
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
                "SELECT w.workspace_id, w.working_dir, w.min_chunk_size, w.max_chunk_size, w.created_at,
                        COALESCE((SELECT COUNT(*) FROM file_refs f WHERE f.workspace_id = w.workspace_id), 0)
                 FROM workspaces w WHERE w.user_id = ?1",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![user_id], |row| {
                    Ok(WorkspaceRow {
                        workspace_id: row.get(0)?,
                        working_dir: row.get(1)?,
                        min_chunk_size: row.get(2)?,
                        max_chunk_size: row.get(3)?,
                        created_at: row.get(4)?,
                        node_count: row.get(5)?,
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
                "SELECT w.workspace_id, w.working_dir, w.min_chunk_size, w.max_chunk_size, w.created_at,
                        COALESCE((SELECT COUNT(*) FROM file_refs f WHERE f.workspace_id = w.workspace_id), 0)
                 FROM workspaces w WHERE w.workspace_id = ?1",
            )?;
            let row = stmt
                .query_row(rusqlite::params![workspace_id], |row| {
                    Ok(WorkspaceRow {
                        workspace_id: row.get(0)?,
                        working_dir: row.get(1)?,
                        min_chunk_size: row.get(2)?,
                        max_chunk_size: row.get(3)?,
                        created_at: row.get(4)?,
                        node_count: row.get(5)?,
                    })
                })
                .ok();
            Ok(row)
        })
        .await?
    }

    /// Deletes a workspace and all its file references.
    pub async fn delete_workspace(&self, workspace_id: &str) -> Result<()> {
        let conn = self.conn.clone();
        let workspace_id = workspace_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
            conn.execute(
                "DELETE FROM file_refs WHERE workspace_id = ?1",
                rusqlite::params![workspace_id],
            )?;
            conn.execute(
                "DELETE FROM workspaces WHERE workspace_id = ?1",
                rusqlite::params![workspace_id],
            )?;
            Ok(())
        })
        .await?
    }
}

/// Returns the current UTC time as an ISO 8601 string.
fn chrono_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Simple ISO 8601 format without external chrono dependency
    format!("{secs}")
}
