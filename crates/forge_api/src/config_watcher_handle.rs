//! Wave C Part 2 — [`ConfigWatcher`] → `ForgeAPI` wiring.
//!
//! This module glues the [`forge_services::ConfigWatcher`] filesystem
//! watcher (Wave C Part 1) to the [`forge_app::fire_config_change_hook`]
//! plugin-hook dispatcher. It lives in `forge_api` rather than
//! `forge_app` because:
//!
//! - `forge_app` is a dependency of `forge_services` (the concrete service
//!   aggregate depends on the app traits), so `forge_app` *cannot* import
//!   `forge_services::ConfigWatcher` without creating a dependency cycle.
//! - The hook dispatcher itself ([`forge_app::hooks::PluginHookHandler`]) is
//!   crate-private to `forge_app`, so callers outside `forge_app` cannot build
//!   the callback directly — they must go through the `fire_config_change_hook`
//!   free function that `forge_app` publicly re-exports.
//!
//! `forge_api` is the natural meeting point: it already depends on both
//! `forge_app` and `forge_services`, so the callback we build here can
//! call `fire_config_change_hook` and the watcher constructor lives on
//! the same side of the dependency graph.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use forge_app::{Services, fire_config_change_hook};
use forge_services::{ConfigChange, ConfigWatcher, RecursiveMode};
use tokio::runtime::Handle;
use tracing::{debug, warn};

/// Cheaply-cloneable handle to the background [`ConfigWatcher`] thread.
///
/// `ForgeAPI` keeps one of these alive for its entire lifetime — the
/// inner `Arc<ConfigWatcher>` owns the `notify-debouncer-full` debouncer
/// whose `Drop` impl stops the watcher thread, so holding the handle is
/// what keeps the watcher running.
///
/// Callers use [`Self::mark_internal_write`] to tell the watcher "I am
/// about to save this file myself, don't fire a `ConfigChange` hook for
/// the next 5 seconds" so Forge's own writes don't round-trip through
/// the plugin-hook system.
///
/// The handle is `Clone` so it can be cached in multiple places
/// (notably inside [`ForgeInfra`] via a callback) without duplicating
/// the underlying watcher.
#[derive(Clone)]
pub struct ConfigWatcherHandle {
    inner: Option<Arc<ConfigWatcher>>,
}

impl ConfigWatcherHandle {
    /// Spawn a new [`ConfigWatcher`] that fires the `ConfigChange`
    /// lifecycle hook on every debounced change under `watch_paths`.
    ///
    /// # Callback design
    ///
    /// `notify-debouncer-full` invokes the callback on a dedicated
    /// background thread that has no tokio runtime attached. The
    /// `fire_config_change_hook` dispatcher is `async`, so we capture
    /// a [`tokio::runtime::Handle`] at construction time and use
    /// `handle.spawn(...)` from inside the closure to schedule each
    /// hook fire on the main runtime. This keeps the watcher thread
    /// non-blocking (the closure returns immediately after scheduling)
    /// and lets the hook run on the same runtime the rest of `ForgeAPI`
    /// uses.
    ///
    /// # Error handling
    ///
    /// - If no tokio runtime is active when `spawn` is called (e.g. in unit
    ///   tests that construct a `ForgeAPI` without `#[tokio::test]`), we log a
    ///   `warn!` and return a no-op handle. The handle is still `Ok(...)` so
    ///   `ForgeAPI::init` does not have to special-case the test path.
    /// - If [`ConfigWatcher::new`] fails (rare — indicates an OS-level `notify`
    ///   setup failure), the error is propagated so the caller can decide
    ///   whether to construct the API anyway.
    pub fn spawn<S: Services + 'static>(
        services: Arc<S>,
        watch_paths: Vec<(PathBuf, RecursiveMode)>,
    ) -> Result<Self> {
        // Grab the current tokio runtime handle so the filesystem
        // callback thread can schedule async work on it. If we are
        // being called outside a tokio context (e.g. from a plain
        // unit test), degrade gracefully to a no-op handle.
        let runtime = match Handle::try_current() {
            Ok(h) => h,
            Err(_) => {
                warn!(
                    "ConfigWatcherHandle::spawn called outside a tokio runtime — \
                     watcher disabled (no hooks will fire for config changes). \
                     This is expected in unit tests."
                );
                return Ok(Self { inner: None });
            }
        };

        // Clone the services aggregate into the filesystem-thread
        // closure. Every dispatch schedules a fresh task on the
        // runtime, so each task needs its own `Arc<S>` clone.
        let services_for_cb = services.clone();
        let callback = move |change: ConfigChange| {
            let services_for_task = services_for_cb.clone();
            debug!(
                source = ?change.source,
                path = %change.file_path.display(),
                "ConfigWatcher callback received change"
            );
            runtime.spawn(async move {
                fire_config_change_hook(services_for_task, change.source, Some(change.file_path))
                    .await;
            });
        };

        let watcher = ConfigWatcher::new(watch_paths, callback)?;
        Ok(Self { inner: Some(Arc::new(watcher)) })
    }

    /// Record that Forge itself is about to write `path`, so the
    /// watcher will suppress any filesystem event that arrives within
    /// the internal-write window (5 seconds — see
    /// `forge_services::config_watcher`).
    ///
    /// No-op if the handle was constructed without an active tokio
    /// runtime (see [`Self::spawn`]).
    ///
    /// The underlying [`ConfigWatcher::mark_internal_write`] is
    /// declared `async` only for API uniformity — its body is a
    /// synchronous mutex lock that never yields. We drive it with
    /// `futures::executor::block_on` so this helper stays sync and
    /// doesn't require any runtime context at the call site.
    pub fn mark_internal_write(&self, path: &Path) {
        if let Some(ref watcher) = self.inner {
            let watcher = watcher.clone();
            let path = path.to_path_buf();
            // `ConfigWatcher::mark_internal_write` is `async` for
            // API uniformity but never yields — it just takes a
            // mutex and inserts into a HashMap. `block_on` drives
            // the future to completion in a single poll.
            futures::executor::block_on(async move {
                watcher.mark_internal_write(path).await;
            });
        }
    }
}
