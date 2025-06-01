use std::path::PathBuf;

use anyhow::Context;
use serde::Serialize;
use serde::de::DeserializeOwned;

#[async_trait::async_trait]
pub trait Cache {
    type Key;
    type Value;
    async fn get(&self, key: Self::Key) -> anyhow::Result<Option<Self::Value>>
    where
        Self: Send + Sync;
    async fn put(&self, key: Self::Key, value: Self::Value) -> anyhow::Result<()>
    where
        Self: Send + Sync;
}

/// A cache for storing and retrieving embeddings on disk using cacache
#[derive(Clone)]
pub struct DiskCache<K, V> {
    path: PathBuf,
    _key: std::marker::PhantomData<K>,
    _value: std::marker::PhantomData<V>,
}

impl<K, V> DiskCache<K, V> {
    pub fn try_new(path: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&path).context("failed to create cache directory")?;
        Ok(Self {
            path,
            _key: std::marker::PhantomData,
            _value: std::marker::PhantomData,
        })
    }
}

#[async_trait::async_trait]
impl<K, T> Cache for DiskCache<K, T>
where
    K: ToString + Send + Sync,
    T: Serialize + DeserializeOwned + Send + Sync,
{
    type Key = K;
    type Value = T;

    async fn get(&self, key: K) -> anyhow::Result<Option<T>> {
        let bytes = cacache::read(&self.path, &key.to_string()).await?;
        let data: T = serde_json::from_slice(&bytes)?;
        Ok(Some(data))
    }

    async fn put(&self, key: K, value: T) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec(&value)?;
        cacache::write(&self.path, &key.to_string(), bytes).await?;
        Ok(())
    }
}
