use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use forge_app::McpCacheRepository;
use forge_app::domain::McpToolCache;
use forge_services::CacheInfra;
use serde::{Deserialize, Serialize};

use super::CacacheRepository;

/// Cache key for MCP tool definitions.
///
/// This key is based on the config hash only, which represents the merged
/// content of both user and local configurations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct McpCacheKey {
    config_hash: String,
}

/// MCP-specific cache repository implementation using cacache.
///
/// This repository implements the `McpCacheRepository` trait using a generic
/// `CacacheRepository` for storage. It uses content-based hashing to store
/// a unified cache for both user and local MCP tools.
///
/// The cache key is derived from the SHA256 hash of the merged configuration,
/// ensuring that changes to either user or local configs invalidate the cache.
pub struct ForgeMcpCacheRepository {
    cache: Arc<CacacheRepository<McpCacheKey, McpToolCache>>,
}

impl ForgeMcpCacheRepository {
    /// TTL for MCP cache entries in seconds (1 hour)
    const TTL_SECONDS: u128 = 3600;

    /// Creates a new MCP cache repository with the specified cache directory.
    ///
    /// The directory will be created if it doesn't exist. All MCP cache data
    /// will be stored under this directory.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache: Arc::new(CacacheRepository::new(cache_dir)) }
    }

    /// Generates a cache key for the given config hash.
    fn make_key(config_hash: &str) -> McpCacheKey {
        McpCacheKey { config_hash: config_hash.to_string() }
    }

    /// Checks if a cache entry is still valid based on TTL.
    ///
    /// Returns true if the cache exists and hasn't expired (< 1 hour old).
    pub async fn is_cache_valid(&self, config_hash: &str) -> Result<bool> {
        let key = Self::make_key(config_hash);

        if let Some(metadata) = self.cache.get_metadata(&key).await? {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();

            let age_ms = now.saturating_sub(metadata.time);
            let age_seconds = age_ms / 1000;

            Ok(age_seconds < Self::TTL_SECONDS)
        } else {
            Ok(false)
        }
    }

    /// Gets the cache age in seconds.
    ///
    /// Returns None if the cache doesn't exist.
    pub async fn get_cache_age_seconds(&self, config_hash: &str) -> Result<Option<u64>> {
        let key = Self::make_key(config_hash);

        if let Some(metadata) = self.cache.get_metadata(&key).await? {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();

            let age_ms = now.saturating_sub(metadata.time);
            Ok(Some((age_ms / 1000) as u64))
        } else {
            Ok(None)
        }
    }
}

#[async_trait::async_trait]
impl McpCacheRepository for ForgeMcpCacheRepository {
    async fn get_cache(&self, config_hash: &str) -> Result<Option<McpToolCache>> {
        let key = Self::make_key(config_hash);
        self.cache.get(&key).await
    }

    async fn set_cache(&self, cache: McpToolCache) -> Result<()> {
        let key = Self::make_key(&cache.config_hash);
        self.cache.set(&key, &cache).await
    }

    async fn clear_cache(&self) -> Result<()> {
        // Clear the entire cache directory
        self.cache.clear().await
    }

    async fn is_cache_valid(&self, config_hash: &str) -> Result<bool> {
        self.is_cache_valid(config_hash).await
    }

    async fn get_cache_age_seconds(&self, config_hash: &str) -> Result<Option<u64>> {
        self.get_cache_age_seconds(config_hash).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_app::domain::ToolDefinition;
    use pretty_assertions::assert_eq;

    use super::*;

    fn test_cache_dir() -> PathBuf {
        tempfile::tempdir().unwrap().into_path()
    }

    fn test_cache_fixture(config_hash: &str) -> McpToolCache {
        let mut tools = BTreeMap::new();
        tools.insert(
            "test_server".to_string(),
            vec![ToolDefinition::new("test_tool").description("A test tool")],
        );

        McpToolCache::new(config_hash.to_string(), tools)
    }

    #[tokio::test]
    async fn test_get_nonexistent_cache() {
        let cache_dir = test_cache_dir();
        let repo = ForgeMcpCacheRepository::new(cache_dir);

        let result = repo.get_cache("nonexistent_hash").await.unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_set_and_get_cache() {
        let cache_dir = test_cache_dir();
        let repo = ForgeMcpCacheRepository::new(cache_dir);

        let cache = test_cache_fixture("test_hash_123");

        repo.set_cache(cache.clone()).await.unwrap();
        let result = repo.get_cache("test_hash_123").await.unwrap();

        assert_eq!(result, Some(cache));
    }

    #[tokio::test]
    async fn test_different_hashes_isolated() {
        let cache_dir = test_cache_dir();
        let repo = ForgeMcpCacheRepository::new(cache_dir);

        let cache1 = test_cache_fixture("hash_1");
        let cache2 = test_cache_fixture("hash_2");

        repo.set_cache(cache1.clone()).await.unwrap();
        repo.set_cache(cache2.clone()).await.unwrap();

        let result1 = repo.get_cache("hash_1").await.unwrap();
        let result2 = repo.get_cache("hash_2").await.unwrap();

        assert_eq!(result1, Some(cache1));
        assert_eq!(result2, Some(cache2));
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let cache_dir = test_cache_dir();
        let repo = ForgeMcpCacheRepository::new(cache_dir);

        let cache1 = test_cache_fixture("hash_1");
        let cache2 = test_cache_fixture("hash_2");

        repo.set_cache(cache1).await.unwrap();
        repo.set_cache(cache2).await.unwrap();

        repo.clear_cache().await.unwrap();

        let result1 = repo.get_cache("hash_1").await.unwrap();
        let result2 = repo.get_cache("hash_2").await.unwrap();

        assert_eq!(result1, None);
        assert_eq!(result2, None);
    }
}
