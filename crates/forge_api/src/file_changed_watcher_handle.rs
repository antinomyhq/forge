//! Phase 7C Wave E-2a — [`FileChangedWatcher`] → `ForgeAPI` wiring.
//!
//! This module glues the [`forge_services::FileChangedWatcher`]
//! filesystem watcher to the [`forge_app::fire_file_changed_hook`]
//! plugin-hook dispatcher. It is the direct sibling of
//! [`crate::config_watcher_handle`] and lives in `forge_api` for the
//! same reason:
//!
//! - `forge_app` is a dependency of `forge_services`, so `forge_app` *cannot*
//!   import `forge_services::FileChangedWatcher` without creating a dependency
//!   cycle.
//! - The hook dispatcher itself ([`forge_app::hooks::PluginHookHandler`]) is
//!   crate-private to `forge_app`, so callers outside `forge_app` cannot build
//!   the callback directly — they must go through the `fire_file_changed_hook`
//!   free function.
//!
//! `forge_api` already depends on both `forge_app` and `forge_services`,
//! so the callback we build here can call `fire_file_changed_hook` and
//! the watcher constructor lives on the same side of the dependency
//! graph.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use forge_app::{Services, fire_file_changed_hook};
use forge_services::{FileChange, FileChangedWatcher, RecursiveMode};
use tokio::runtime::Handle;
use tracing::{debug, warn};

/// Cheaply-cloneable handle to the background [`FileChangedWatcher`]
/// thread.
///
/// `ForgeAPI` keeps one of these alive for its entire lifetime — the
/// inner `Arc<FileChangedWatcher>` owns the `notify-debouncer-full`
/// debouncer whose `Drop` impl stops the watcher thread, so holding
/// the handle is what keeps the watcher running.
///
/// The handle is `Clone` so it can be cached in multiple places
/// without duplicating the underlying watcher.
#[derive(Clone)]
pub struct FileChangedWatcherHandle {
    inner: Option<Arc<FileChangedWatcher>>,
}

impl FileChangedWatcherHandle {
    /// Spawn a new [`FileChangedWatcher`] that fires the `FileChanged`
    /// lifecycle hook on every debounced change under `watch_paths`.
    ///
    /// # Callback design
    ///
    /// `notify-debouncer-full` invokes the callback on a dedicated
    /// background thread that has no tokio runtime attached. The
    /// [`fire_file_changed_hook`] dispatcher is `async`, so we capture
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
    /// - If [`FileChangedWatcher::new`] fails (rare — indicates an OS-level
    ///   `notify` setup failure), the error is propagated so the caller can
    ///   decide whether to construct the API anyway.
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
                    "FileChangedWatcherHandle::spawn called outside a tokio runtime — \
                     watcher disabled (no hooks will fire for file changes). \
                     This is expected in unit tests."
                );
                return Ok(Self { inner: None });
            }
        };

        // Clone the services aggregate into the filesystem-thread
        // closure. Every dispatch schedules a fresh task on the
        // runtime, so each task needs its own `Arc<S>` clone.
        let services_for_cb = services.clone();
        let callback = move |change: FileChange| {
            let services_for_task = services_for_cb.clone();
            debug!(
                path = %change.file_path.display(),
                event = ?change.event,
                "FileChangedWatcher callback received change"
            );
            runtime.spawn(async move {
                fire_file_changed_hook(services_for_task, change.file_path, change.event).await;
            });
        };

        let watcher = FileChangedWatcher::new(watch_paths, callback)?;
        Ok(Self { inner: Some(Arc::new(watcher)) })
    }

    /// Record that Forge itself is about to write `path`, so the
    /// watcher will suppress any filesystem event that arrives within
    /// the internal-write window (5 seconds).
    ///
    /// # Reserved for future use
    ///
    /// No caller inside `forge_api` currently invokes this method:
    /// Wave E-2a is strictly read-only observability, and Forge does
    /// not yet write to any of the files the `FileChangedWatcher`
    /// observes. The method is exposed now so the companion
    /// Wave E-2a-cwd work can wire up `.envrc` / `.env` mutation
    /// suppression without having to touch this file again.
    ///
    /// No-op if the handle was constructed without an active tokio
    /// runtime (see [`Self::spawn`]).
    ///
    /// The underlying [`FileChangedWatcher::mark_internal_write`] is
    /// declared `async` only for API uniformity — its body is a
    /// synchronous mutex lock that never yields. We drive it with
    /// `futures::executor::block_on` so this helper stays sync and
    /// doesn't require any runtime context at the call site.
    pub fn mark_internal_write(&self, path: &Path) {
        if let Some(ref watcher) = self.inner {
            let watcher = watcher.clone();
            let path = path.to_path_buf();
            // `FileChangedWatcher::mark_internal_write` is `async`
            // for API uniformity but never yields — it just takes a
            // mutex and inserts into a HashMap. `block_on` drives
            // the future to completion in a single poll.
            futures::executor::block_on(async move {
                watcher.mark_internal_write(path).await;
            });
        }
    }

    /// Install additional runtime watchers over the given paths.
    ///
    /// Proxies directly to [`FileChangedWatcher::add_paths`]. Callers
    /// are responsible for expanding any pipe-separated hook matcher
    /// strings (e.g. `.envrc|.env`) into individual `(PathBuf,
    /// RecursiveMode)` pairs before calling — [`parse_file_changed_matcher`]
    /// is the shared helper both the startup resolver and the runtime
    /// consumer use for that step.
    ///
    /// No-op if the handle was constructed with `inner: None` (e.g.
    /// [`Self::spawn`] degraded to a no-op because no tokio runtime
    /// was active).
    ///
    /// # Errors
    ///
    /// This method cannot fail: per-path install failures are logged
    /// inside [`FileChangedWatcher::add_paths`] at `debug` level and
    /// silently dropped, matching the observability-only nature of
    /// the `FileChanged` lifecycle event.
    pub fn add_paths(&self, watch_paths: Vec<(PathBuf, RecursiveMode)>) {
        if let Some(ref watcher) = self.inner {
            watcher.add_paths(watch_paths);
        }
    }
}

/// Implement [`FileChangedWatcherOps`] so the orchestrator can receive
/// a late-bound, concrete accessor via the
/// [`forge_app::install_file_changed_watcher_ops`] hand-off (see
/// `ForgeAPI::init`). The trait lives in `forge_app` to avoid
/// coupling the orchestrator to `forge_api`'s concrete handle type —
/// this impl is the bridge that makes the two crates fit together
/// without creating a dependency cycle.
impl forge_app::FileChangedWatcherOps for FileChangedWatcherHandle {
    fn add_paths(&self, watch_paths: Vec<(PathBuf, RecursiveMode)>) {
        FileChangedWatcherHandle::add_paths(self, watch_paths);
    }
}

/// Parse a hook-config `FileChanged` matcher string into
/// `(PathBuf, RecursiveMode)` pairs, splitting on `|` to support
/// alternatives (e.g. `".envrc|.env"`) and resolving relative
/// entries against `base_cwd`.
///
/// Returns every parsed pair verbatim — existence filtering is the
/// caller's responsibility. The startup resolver
/// (`resolve_file_changed_watch_paths`) filters out paths that do
/// not exist on disk to keep the install log quiet; the runtime
/// consumer (Wave E-2b orchestrator dispatch) deliberately does
/// **not** filter, because a freshly-returned `watch_paths` entry
/// from a `SessionStart` hook may intentionally point at a file the
/// hook is about to create.
///
/// Entries with empty/whitespace-only alternatives are silently
/// dropped. All entries are assigned
/// [`RecursiveMode::NonRecursive`] because Claude Code's wire
/// semantics treat each matcher as a single file, not a directory
/// tree.
pub(crate) fn parse_file_changed_matcher(
    matcher: &str,
    base_cwd: &Path,
) -> Vec<(PathBuf, RecursiveMode)> {
    matcher
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|alternative| {
            let candidate = Path::new(alternative);
            let resolved = if candidate.is_absolute() {
                candidate.to_path_buf()
            } else {
                base_cwd.join(candidate)
            };
            (resolved, RecursiveMode::NonRecursive)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_changed_matcher_single_path_relative_resolves_to_cwd() {
        let base = PathBuf::from("/workspace/project");
        let fixture = parse_file_changed_matcher(".envrc", &base);
        let expected = vec![(
            PathBuf::from("/workspace/project/.envrc"),
            RecursiveMode::NonRecursive,
        )];
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_parse_file_changed_matcher_pipe_separated_splits_all_alternatives() {
        let base = PathBuf::from("/workspace/project");
        let fixture = parse_file_changed_matcher(".envrc|.env| .env.local ", &base);
        let expected = vec![
            (
                PathBuf::from("/workspace/project/.envrc"),
                RecursiveMode::NonRecursive,
            ),
            (
                PathBuf::from("/workspace/project/.env"),
                RecursiveMode::NonRecursive,
            ),
            (
                PathBuf::from("/workspace/project/.env.local"),
                RecursiveMode::NonRecursive,
            ),
        ];
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_parse_file_changed_matcher_absolute_path_not_resolved() {
        let base = PathBuf::from("/workspace/project");
        let fixture = parse_file_changed_matcher("/etc/hosts|relative.txt", &base);
        let expected = vec![
            (PathBuf::from("/etc/hosts"), RecursiveMode::NonRecursive),
            (
                PathBuf::from("/workspace/project/relative.txt"),
                RecursiveMode::NonRecursive,
            ),
        ];
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_parse_file_changed_matcher_empty_and_whitespace_alternatives_dropped() {
        let base = PathBuf::from("/workspace/project");
        let fixture = parse_file_changed_matcher("|.envrc||   |.env|", &base);
        let expected = vec![
            (
                PathBuf::from("/workspace/project/.envrc"),
                RecursiveMode::NonRecursive,
            ),
            (
                PathBuf::from("/workspace/project/.env"),
                RecursiveMode::NonRecursive,
            ),
        ];
        assert_eq!(fixture, expected);
    }
}
