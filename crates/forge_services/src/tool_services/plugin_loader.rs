use std::sync::Arc;

use forge_app::PluginLoader;
use forge_domain::{LoadedPlugin, PluginLoadResult, PluginRepository};
use tokio::sync::RwLock;

/// In-process plugin loader that caches discovery results.
///
/// Wraps a [`PluginRepository`] (typically `ForgePluginRepository`) and
/// memoises its output in an `RwLock<Option<Arc<PluginLoadResult>>>`.
///
/// Mirrors `ForgeSkillFetch` — the first call scans the filesystem, later
/// calls return a cheap `Arc::clone` of the cached result. Callers can
/// drop the cache via [`invalidate_cache`](PluginLoader::invalidate_cache)
/// (invoked by `:plugin reload` / `:plugin enable` / `:plugin disable`
/// once Phase 9 lands).
///
/// ## Error surfacing
///
/// The cache stores the full [`PluginLoadResult`] — both successful plugins
/// and per-plugin load errors — so consumers that want a "broken plugin"
/// list (e.g. Phase 9's `:plugin list`) can pull diagnostics via
/// [`list_plugins_with_errors`](PluginLoader::list_plugins_with_errors)
/// without performing a second scan. The classic
/// [`list_plugins`](PluginLoader::list_plugins) entry point stays lossy
/// for backward compatibility.
///
/// ## Why not memoise inside `ForgePluginRepository`?
///
/// Keeping the repository stateless makes it trivially testable with
/// temporary directories and keeps the I/O layer honest (every call hits
/// disk). The service layer is the correct place to trade freshness for
/// speed.
pub struct ForgePluginLoader<R> {
    repository: Arc<R>,
    /// In-memory cache of the last discovery pass.
    ///
    /// We store `Arc<PluginLoadResult>` (rather than the value directly)
    /// so that the two public accessors can each return a cheap clone
    /// without holding the read lock for the duration of the caller's
    /// work. Callers that mutate the returned vectors only touch their
    /// own clone.
    cache: RwLock<Option<Arc<PluginLoadResult>>>,
}

impl<R> ForgePluginLoader<R> {
    /// Creates a new plugin loader wrapping `repository`.
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository, cache: RwLock::new(None) }
    }

    /// Returns a cached `Arc<PluginLoadResult>` or loads it from the
    /// repository on first call.
    ///
    /// Uses double-checked locking: a cheap read-lock fast path, falling
    /// back to an expensive write-lock slow path when the cache is empty.
    async fn get_or_load(&self) -> anyhow::Result<Arc<PluginLoadResult>>
    where
        R: PluginRepository,
    {
        // Fast path: read lock, clone Arc if populated.
        {
            let guard = self.cache.read().await;
            if let Some(result) = guard.as_ref() {
                return Ok(Arc::clone(result));
            }
        }

        // Slow path: write lock, re-check, load.
        let mut guard = self.cache.write().await;
        if let Some(result) = guard.as_ref() {
            return Ok(Arc::clone(result));
        }

        let result = Arc::new(self.repository.load_plugins_with_errors().await?);
        *guard = Some(Arc::clone(&result));
        Ok(result)
    }
}

#[async_trait::async_trait]
impl<R: PluginRepository + Send + Sync + 'static> PluginLoader for ForgePluginLoader<R> {
    async fn list_plugins(&self) -> anyhow::Result<Vec<LoadedPlugin>> {
        let result = self.get_or_load().await?;
        Ok(result.plugins.clone())
    }

    async fn list_plugins_with_errors(&self) -> anyhow::Result<PluginLoadResult> {
        let result = self.get_or_load().await?;
        Ok((*result).clone())
    }

    async fn invalidate_cache(&self) {
        let mut guard = self.cache.write().await;
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_domain::{
        LoadedPlugin, PluginLoadError, PluginLoadErrorKind, PluginLoadResult, PluginRepository,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    /// Test repository that counts how many times `load_plugins_with_errors`
    /// was invoked and returns a mutable [`PluginLoadResult`].
    struct MockPluginRepository {
        result: Mutex<PluginLoadResult>,
        load_calls: Mutex<u32>,
    }

    impl MockPluginRepository {
        fn with_plugins(plugins: Vec<LoadedPlugin>) -> Self {
            Self {
                result: Mutex::new(PluginLoadResult::new(plugins, Vec::new())),
                load_calls: Mutex::new(0),
            }
        }

        fn with_result(result: PluginLoadResult) -> Self {
            Self { result: Mutex::new(result), load_calls: Mutex::new(0) }
        }

        fn load_call_count(&self) -> u32 {
            *self.load_calls.lock().unwrap()
        }

        fn set_plugins(&self, plugins: Vec<LoadedPlugin>) {
            *self.result.lock().unwrap() = PluginLoadResult::new(plugins, Vec::new());
        }
    }

    #[async_trait::async_trait]
    impl PluginRepository for MockPluginRepository {
        async fn load_plugins(&self) -> anyhow::Result<Vec<LoadedPlugin>> {
            // Delegate through the error-surfacing variant so the mock
            // exercises the same path as production code.
            self.load_plugins_with_errors().await.map(|r| r.plugins)
        }

        async fn load_plugins_with_errors(&self) -> anyhow::Result<PluginLoadResult> {
            *self.load_calls.lock().unwrap() += 1;
            Ok(self.result.lock().unwrap().clone())
        }
    }

    fn fixture_plugin(name: &str) -> LoadedPlugin {
        use std::path::PathBuf;

        use forge_domain::{PluginManifest, PluginSource};

        LoadedPlugin {
            name: name.to_string(),
            path: PathBuf::from(format!("/fake/{name}")),
            manifest: PluginManifest { name: Some(name.to_string()), ..Default::default() },
            source: PluginSource::Global,
            enabled: true,
            is_builtin: false,
            commands_paths: Vec::new(),
            agents_paths: Vec::new(),
            skills_paths: Vec::new(),
            mcp_servers: None,
        }
    }

    fn fixture_load_error(name: &str, err: &str) -> PluginLoadError {
        use std::path::PathBuf;
        PluginLoadError {
            plugin_name: Some(name.to_string()),
            path: PathBuf::from(format!("/fake/{name}")),
            kind: PluginLoadErrorKind::Other,
            error: err.to_string(),
        }
    }

    #[tokio::test]
    async fn test_list_plugins_first_call_reads_repository() {
        let repo = Arc::new(MockPluginRepository::with_plugins(vec![fixture_plugin(
            "alpha",
        )]));
        let loader = ForgePluginLoader::new(repo.clone());

        let actual = loader.list_plugins().await.unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].name, "alpha");
        assert_eq!(repo.load_call_count(), 1);
    }

    #[tokio::test]
    async fn test_list_plugins_second_call_returns_cached() {
        let repo = Arc::new(MockPluginRepository::with_plugins(vec![
            fixture_plugin("alpha"),
            fixture_plugin("beta"),
        ]));
        let loader = ForgePluginLoader::new(repo.clone());

        let first = loader.list_plugins().await.unwrap();
        let second = loader.list_plugins().await.unwrap();

        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 2);
        // Repository was only hit once despite two calls.
        assert_eq!(repo.load_call_count(), 1);
    }

    #[tokio::test]
    async fn test_invalidate_cache_forces_reload() {
        let repo = Arc::new(MockPluginRepository::with_plugins(vec![fixture_plugin(
            "alpha",
        )]));
        let loader = ForgePluginLoader::new(repo.clone());

        // First call populates cache.
        let _ = loader.list_plugins().await.unwrap();
        assert_eq!(repo.load_call_count(), 1);

        // Invalidate and verify the next call re-reads.
        loader.invalidate_cache().await;
        let _ = loader.list_plugins().await.unwrap();
        assert_eq!(repo.load_call_count(), 2);
    }

    #[tokio::test]
    async fn test_invalidate_cache_surfaces_new_plugins() {
        let repo = Arc::new(MockPluginRepository::with_plugins(vec![fixture_plugin(
            "alpha",
        )]));
        let loader = ForgePluginLoader::new(repo.clone());

        // Cache the initial state.
        let before = loader.list_plugins().await.unwrap();
        assert_eq!(before.len(), 1);

        // Simulate a new plugin landing on disk mid-session.
        repo.set_plugins(vec![fixture_plugin("alpha"), fixture_plugin("beta")]);

        // Without invalidation, we still see the cached snapshot.
        let stale = loader.list_plugins().await.unwrap();
        assert_eq!(stale.len(), 1);

        // After invalidation, the new plugin surfaces.
        loader.invalidate_cache().await;
        let fresh = loader.list_plugins().await.unwrap();
        assert_eq!(fresh.len(), 2);
        assert_eq!(fresh[1].name, "beta");
    }

    #[tokio::test]
    async fn test_list_plugins_with_errors_surfaces_broken_plugins() {
        // Fixture: one healthy plugin and one broken one.
        let repo = Arc::new(MockPluginRepository::with_result(PluginLoadResult::new(
            vec![fixture_plugin("alpha")],
            vec![fixture_load_error("broken", "missing name")],
        )));
        let loader = ForgePluginLoader::new(repo.clone());

        let actual = loader.list_plugins_with_errors().await.unwrap();

        assert_eq!(actual.plugins.len(), 1);
        assert_eq!(actual.plugins[0].name, "alpha");
        assert_eq!(actual.errors.len(), 1);
        assert_eq!(actual.errors[0].plugin_name.as_deref(), Some("broken"));
        assert!(actual.has_errors());
    }

    #[tokio::test]
    async fn test_list_plugins_and_with_errors_share_cache() {
        // A mixed call pattern must hit the repository exactly once.
        let repo = Arc::new(MockPluginRepository::with_result(PluginLoadResult::new(
            vec![fixture_plugin("alpha")],
            vec![fixture_load_error("broken", "bad json")],
        )));
        let loader = ForgePluginLoader::new(repo.clone());

        // First: call the lossy variant to populate the cache.
        let lossy = loader.list_plugins().await.unwrap();
        assert_eq!(lossy.len(), 1);
        assert_eq!(repo.load_call_count(), 1);

        // Second: call the error-surfacing variant and expect the cache
        // to be reused (no additional repository hit).
        let full = loader.list_plugins_with_errors().await.unwrap();
        assert_eq!(full.plugins.len(), 1);
        assert_eq!(full.errors.len(), 1);
        assert_eq!(repo.load_call_count(), 1);
    }

    #[tokio::test]
    async fn test_list_plugins_hides_errors_from_legacy_callers() {
        // Errors must not leak into the lossy `list_plugins()` entry
        // point; only `list_plugins_with_errors()` exposes them.
        let repo = Arc::new(MockPluginRepository::with_result(PluginLoadResult::new(
            vec![fixture_plugin("alpha")],
            vec![fixture_load_error("broken", "bad json")],
        )));
        let loader = ForgePluginLoader::new(repo.clone());

        let plugins = loader.list_plugins().await.unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "alpha");
    }
}
