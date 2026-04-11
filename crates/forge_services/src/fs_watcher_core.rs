//! Shared filesystem-watcher primitives used by both [`ConfigWatcher`]
//! and [`FileChangedWatcher`].
//!
//! This module factors out the timing constants, the path canonicalization
//! helper, and the synchronous internal-write probe that were originally
//! private to [`crate::config_watcher`]. Hoisting them here lets the
//! Phase 7C `FileChangedWatcher` reuse the exact same debounce /
//! atomic-save / suppression semantics without code duplication.
//!
//! All items are `pub(crate)` — the public `ConfigWatcher` /
//! `FileChangedWatcher` types re-expose whatever surface they need.
//!
//! [`ConfigWatcher`]: crate::config_watcher::ConfigWatcher
//! [`FileChangedWatcher`]: crate::file_changed_watcher::FileChangedWatcher

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Re-export of `notify::RecursiveMode` so the in-crate watchers can
/// reference it without depending on `notify_debouncer_full::notify`
/// directly. External callers still get the re-export via
/// `crate::config_watcher`.
pub(crate) use notify_debouncer_full::notify::RecursiveMode;

/// How long after a `mark_internal_write` call the path stays
/// suppressed. Matches Claude Code's 5-second window.
pub(crate) const INTERNAL_WRITE_WINDOW: Duration = Duration::from_secs(5);

/// How long a watcher waits after a `Remove` event before firing a
/// delete. If a matching `Create` arrives within this window the pair
/// is collapsed into a single `Modify`-equivalent event. Matches the
/// 1.7-second grace period documented in
/// `claude-code/src/utils/settings/changeDetector.ts`.
pub(crate) const ATOMIC_SAVE_GRACE: Duration = Duration::from_millis(1700);

/// Debounce timeout handed to `notify-debouncer-full`. Matches Claude
/// Code's `awaitWriteFinish.stabilityThreshold: 1000`.
pub(crate) const DEBOUNCE_TIMEOUT: Duration = Duration::from_secs(1);

/// Minimum interval between back-to-back dispatches for the same path.
///
/// `notify-debouncer-full` coalesces raw filesystem events but still
/// emits multi-event batches for a single atomic save (e.g.
/// `[Remove, Create, Modify, Modify]` on macOS FSEvents). Without a
/// callback-level per-path cooldown we would fire the user's
/// callback multiple times for one save. We use a window slightly
/// larger than [`DEBOUNCE_TIMEOUT`] so every event inside one
/// debounce batch collapses to a single dispatch.
pub(crate) const DISPATCH_COOLDOWN: Duration = Duration::from_millis(1500);

/// Canonicalize `path` for map lookup purposes. Uses
/// [`std::fs::canonicalize`] when the path exists (resolves symlinks
/// like macOS's `/var → /private/var`) and falls back to the
/// un-canonicalized path when it does not (e.g. after a delete, or
/// for a path that has not been created yet). This keeps the
/// internal-write and pending-unlink maps keyed consistently with the
/// paths emitted by `notify-debouncer-full`.
pub(crate) fn canonicalize_for_lookup(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Returns `true` if `path` was marked as an internal write within the
/// last [`INTERNAL_WRITE_WINDOW`]. Synchronous helper so the debouncer
/// callback can call it without needing a tokio runtime. Checks both
/// the as-received path and its canonicalized form so callers can pass
/// either.
pub(crate) fn is_internal_write_sync(
    recent: &Mutex<HashMap<PathBuf, Instant>>,
    path: &Path,
) -> bool {
    let guard = recent
        .lock()
        .expect("recent_internal_writes mutex poisoned");
    let hit = |p: &Path| {
        guard
            .get(p)
            .map(|ts| ts.elapsed() < INTERNAL_WRITE_WINDOW)
            .unwrap_or(false)
    };
    if hit(path) {
        return true;
    }
    let canonical = canonicalize_for_lookup(path);
    hit(&canonical)
}
