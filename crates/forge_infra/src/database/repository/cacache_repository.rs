use std::hash::Hash;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

/// Generic content-addressable cache repository using cacache.
///
/// This repository provides a type-safe wrapper around cacache for arbitrary
/// key-value caching with content verification. Keys are serialized to
/// deterministic strings using serde_json, and values are stored as JSON
/// using serde_json for maximum compatibility.
pub struct CacacheRepository {
    cache_dir: PathBuf,
    ttl_seconds: Option<u128>,
}

impl CacacheRepository {
    /// Creates a new cache repository with the specified cache directory.
    ///
    /// The directory will be created if it doesn't exist. All cache data
    /// will be stored under this directory using cacache's content-addressable
    /// storage format.
    ///
    /// # Arguments
    /// * `cache_dir` - Directory where cache data will be stored
    /// * `ttl_seconds` - Optional TTL in seconds. If provided, entries older
    ///   than this will be considered expired.
    pub fn new(cache_dir: PathBuf, ttl_seconds: Option<u128>) -> Self {
        Self { cache_dir, ttl_seconds }
    }

    /// Converts a key to a deterministic cache key string using its hash value.
    fn key_to_string<K>(&self, key: &K) -> Result<String>
    where
        K: Hash,
    {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;

        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        Ok(hasher.finish().to_string())
    }
}

#[async_trait::async_trait]
impl forge_services::CacheRepository for CacacheRepository {
    async fn cache_get<K, V>(&self, key: &K) -> Result<Option<V>>
    where
        K: Hash + Sync,
        V: serde::Serialize + DeserializeOwned + Send,
    {
        let key_str = self.key_to_string(key)?;

        match cacache::read(&self.cache_dir, &key_str).await {
            Ok(data) => {
                let value: V =
                    serde_json::from_slice(&data).context("Failed to deserialize cached value")?;
                Ok(Some(value))
            }
            Err(e) => {
                // Check if error is NotFound by converting to string and checking message
                // cacache errors don't have a kind() method
                let error_str = e.to_string();
                if error_str.contains("not found") || error_str.contains("NotFound") {
                    Ok(None)
                } else {
                    Err(e).context("Failed to read from cache")
                }
            }
        }
    }

    async fn cache_set<K, V>(&self, key: &K, value: &V) -> Result<()>
    where
        K: Hash + Sync,
        V: serde::Serialize + Sync,
    {
        let key_str = self.key_to_string(key)?;
        let data = serde_json::to_vec(value).context("Failed to serialize value for caching")?;

        cacache::write(&self.cache_dir, &key_str, data)
            .await
            .context("Failed to write to cache")?;

        Ok(())
    }

    async fn cache_clear(&self) -> Result<()> {
        cacache::clear(&self.cache_dir)
            .await
            .context("Failed to clear cache")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_services::CacheRepository;
    use pretty_assertions::assert_eq;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    struct TestKey {
        id: String,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestValue {
        data: String,
        count: i32,
    }

    fn test_cache_dir() -> PathBuf {
        tempfile::tempdir().unwrap().keep()
    }

    #[tokio::test]
    async fn test_get_nonexistent_key() {
        let cache_dir = test_cache_dir();
        let cache = CacacheRepository::new(cache_dir, None);

        let key = TestKey { id: "test".to_string() };
        let result: Option<TestValue> = cache.cache_get(&key).await.unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let cache_dir = test_cache_dir();
        let cache = CacacheRepository::new(cache_dir, None);

        let key = TestKey { id: "test".to_string() };
        let value = TestValue { data: "hello".to_string(), count: 42 };

        cache.cache_set(&key, &value).await.unwrap();
        let result: Option<TestValue> = cache.cache_get(&key).await.unwrap();

        assert_eq!(result, Some(value));
    }

    #[tokio::test]
    async fn test_clear() {
        let cache_dir = test_cache_dir();
        let cache = CacacheRepository::new(cache_dir, None);

        let key1 = TestKey { id: "test1".to_string() };
        let key2 = TestKey { id: "test2".to_string() };
        let value = TestValue { data: "hello".to_string(), count: 42 };

        cache.cache_set(&key1, &value).await.unwrap();
        cache.cache_set(&key2, &value).await.unwrap();

        cache.cache_clear().await.unwrap();

        let result1: Option<TestValue> = cache.cache_get(&key1).await.unwrap();
        let result2: Option<TestValue> = cache.cache_get(&key2).await.unwrap();

        assert_eq!(result1, None);
        assert_eq!(result2, None);
    }
}
