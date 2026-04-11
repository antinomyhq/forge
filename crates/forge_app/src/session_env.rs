//! Session-scoped environment variable cache populated by hook env files.
//!
//! When a lifecycle hook writes `export KEY=VALUE` lines to the file
//! pointed to by `FORGE_ENV_FILE`, this cache captures those variables
//! and makes them available to subsequent BashTool / Shell invocations.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;

/// Thread-safe cache of environment variables harvested from hook env files.
#[derive(Debug, Clone, Default)]
pub struct SessionEnvCache {
    vars: Arc<RwLock<HashMap<String, String>>>,
}

impl SessionEnvCache {
    pub fn new() -> Self {
        Self { vars: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Read a hook env file and merge any `export KEY=VALUE` or `KEY=VALUE`
    /// lines into the cache. Duplicate keys are overwritten (last-write-wins).
    pub async fn ingest_env_file(&self, path: &Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let content = tokio::fs::read_to_string(path).await?;
        let mut guard = self.vars.write().await;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Support both `export KEY=VALUE` and `KEY=VALUE`
            let line = line.strip_prefix("export ").unwrap_or(line);
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                // Strip surrounding quotes if present
                let value = value
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .unwrap_or(value);
                let value = value
                    .strip_prefix('\'')
                    .and_then(|v| v.strip_suffix('\''))
                    .unwrap_or(value);
                if !key.is_empty() {
                    guard.insert(key.to_string(), value.to_string());
                }
            }
        }
        Ok(())
    }

    /// Get all cached environment variables.
    pub async fn get_vars(&self) -> HashMap<String, String> {
        self.vars.read().await.clone()
    }

    /// Clear all cached variables (called on session end).
    pub async fn clear(&self) {
        self.vars.write().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[tokio::test]
    async fn test_ingest_env_file_parses_export_lines() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "export FOO=bar").unwrap();
        writeln!(tmp, "export BAZ=qux").unwrap();

        let cache = SessionEnvCache::new();
        cache.ingest_env_file(tmp.path()).await.unwrap();

        let vars = cache.get_vars().await;
        assert_eq!(vars.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(vars.get("BAZ").map(String::as_str), Some("qux"));
    }

    #[tokio::test]
    async fn test_ingest_env_file_parses_bare_key_value() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "MY_VAR=hello").unwrap();
        writeln!(tmp, "OTHER=world").unwrap();

        let cache = SessionEnvCache::new();
        cache.ingest_env_file(tmp.path()).await.unwrap();

        let vars = cache.get_vars().await;
        assert_eq!(vars.get("MY_VAR").map(String::as_str), Some("hello"));
        assert_eq!(vars.get("OTHER").map(String::as_str), Some("world"));
    }

    #[tokio::test]
    async fn test_ingest_env_file_strips_quotes() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "export DQ=\"double quoted\"").unwrap();
        writeln!(tmp, "export SQ='single quoted'").unwrap();
        writeln!(tmp, "BARE=no quotes").unwrap();

        let cache = SessionEnvCache::new();
        cache.ingest_env_file(tmp.path()).await.unwrap();

        let vars = cache.get_vars().await;
        assert_eq!(vars.get("DQ").map(String::as_str), Some("double quoted"));
        assert_eq!(vars.get("SQ").map(String::as_str), Some("single quoted"));
        assert_eq!(vars.get("BARE").map(String::as_str), Some("no quotes"));
    }

    #[tokio::test]
    async fn test_ingest_env_file_handles_missing_file() {
        let cache = SessionEnvCache::new();
        let result = cache
            .ingest_env_file(Path::new("/tmp/nonexistent-forge-env-file-12345"))
            .await;
        assert!(result.is_ok());
        assert!(cache.get_vars().await.is_empty());
    }

    #[tokio::test]
    async fn test_clear_empties_cache() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "export KEY=value").unwrap();

        let cache = SessionEnvCache::new();
        cache.ingest_env_file(tmp.path()).await.unwrap();
        assert!(!cache.get_vars().await.is_empty());

        cache.clear().await;
        assert!(cache.get_vars().await.is_empty());
    }

    #[tokio::test]
    async fn test_ingest_env_file_skips_comments_and_empty_lines() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "# This is a comment").unwrap();
        writeln!(tmp).unwrap();
        writeln!(tmp, "export REAL=value").unwrap();
        writeln!(tmp, "  # indented comment").unwrap();

        let cache = SessionEnvCache::new();
        cache.ingest_env_file(tmp.path()).await.unwrap();

        let vars = cache.get_vars().await;
        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("REAL").map(String::as_str), Some("value"));
    }

    #[tokio::test]
    async fn test_ingest_env_file_last_write_wins() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "export KEY=first").unwrap();
        writeln!(tmp, "export KEY=second").unwrap();

        let cache = SessionEnvCache::new();
        cache.ingest_env_file(tmp.path()).await.unwrap();

        let vars = cache.get_vars().await;
        assert_eq!(vars.get("KEY").map(String::as_str), Some("second"));
    }
}
