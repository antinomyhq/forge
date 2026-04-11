//! Filesystem watcher for the `FileChanged` plugin hook.
//!
//! This is the Phase 7C Wave E-2a sibling of [`crate::config_watcher`].
//! Where [`ConfigWatcher`] observes Forge's own config directories and
//! classifies every event into a [`forge_domain::ConfigSource`], this
//! watcher observes an arbitrary list of user-requested paths pulled
//! from the merged `hooks.json` config and emits raw
//! [`forge_domain::FileChangeEvent`] values.
//!
//! # Semantics
//!
//! The runtime semantics are identical to [`ConfigWatcher`]:
//!
//! - a 1-second debounce window (`notify-debouncer-full`),
//! - 5-second internal-write suppression so Forge's own writes never round-trip
//!   through the hook system,
//! - a 1.7-second atomic-save grace period that collapses the `Remove → Create`
//!   pair editors emit during an atomic save into a single `Change` event,
//! - per-path dispatch cooldown (1.5 s) so multi-event batches (the `[Remove,
//!   Create, Modify, Modify]` storm macOS FSEvents emits for one atomic save)
//!   collapse to a single user-visible callback.
//!
//! All timing constants live in [`crate::fs_watcher_core`] and are shared
//! byte-for-byte with [`ConfigWatcher`].
//!
//! # Event kind mapping
//!
//! | `notify::EventKind`  | [`FileChangeEvent`]                      |
//! |----------------------|------------------------------------------|
//! | `Create(_)`          | `Add` (or `Change` if collapsing a save) |
//! | `Modify(_)`          | `Change`                                 |
//! | `Remove(_)`          | `Unlink` (after grace period)            |
//! | `Access(_)`          | ignored                                  |
//! | `Any` / `Other`      | ignored                                  |
//!
//! [`ConfigWatcher`]: crate::config_watcher::ConfigWatcher

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use forge_domain::FileChangeEvent;
use notify_debouncer_full::notify::{self, EventKind, RecommendedWatcher};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::fs_watcher_core::{
    ATOMIC_SAVE_GRACE, DEBOUNCE_TIMEOUT, DISPATCH_COOLDOWN, RecursiveMode, canonicalize_for_lookup,
    is_internal_write_sync,
};

/// A debounced filesystem change detected by [`FileChangedWatcher`].
///
/// The shape matches Claude Code's `FileChanged` wire event: a path
/// plus a `(change | add | unlink)` discriminator. The orchestrator
/// wraps this in a [`forge_domain::FileChangedPayload`] and fires the
/// `FileChanged` lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Absolute path of the file that changed.
    pub file_path: PathBuf,
    /// Kind of change (add / change / unlink).
    pub event: FileChangeEvent,
}

/// Internal state shared between [`FileChangedWatcher`] and the
/// debouncer callback thread. Only holds `Arc`/`Mutex` types so it is
/// trivially `Send + Sync + 'static`, which is required for the
/// `notify-debouncer-full` event handler closure.
struct FileChangedWatcherState {
    /// User-supplied callback invoked once per debounced
    /// [`FileChange`].
    callback: Arc<dyn Fn(FileChange) + Send + Sync>,

    /// Map of paths Forge just wrote → instant the write was recorded.
    /// Consulted on every event so events triggered by Forge's own
    /// saves are suppressed for the internal-write window (see
    /// [`crate::fs_watcher_core::INTERNAL_WRITE_WINDOW`]).
    recent_internal_writes: Arc<Mutex<HashMap<PathBuf, Instant>>>,

    /// Map of paths that just saw a `Remove` event → instant the
    /// remove was recorded. Used by the atomic-save grace period to
    /// collapse `unlink → add` pairs into a single `Change` event.
    pending_unlinks: Arc<Mutex<HashMap<PathBuf, Instant>>>,

    /// Map of paths → instant of the last successful dispatch. Used
    /// by [`fire_change`] to collapse multi-event batches (e.g. the
    /// `[Remove, Create, Modify]` storm macOS emits for an atomic
    /// save) into a single user-visible callback invocation per
    /// [`DISPATCH_COOLDOWN`] window.
    last_fired: Arc<Mutex<HashMap<PathBuf, Instant>>>,
}

/// Filesystem watcher for the `FileChanged` lifecycle hook.
///
/// Install one of these per running `ForgeAPI`, passing in the list
/// of watch paths extracted from the merged hook config. The
/// user-supplied callback is invoked once per debounced, cooldown-
/// collapsed, internal-write-filtered [`FileChange`].
pub struct FileChangedWatcher {
    /// Shared internal-write map. Exposed via
    /// [`mark_internal_write`](Self::mark_internal_write) /
    /// [`is_internal_write`](Self::is_internal_write) and handed to
    /// the debouncer callback via [`FileChangedWatcherState`].
    recent_internal_writes: Arc<Mutex<HashMap<PathBuf, Instant>>>,

    /// Holds the live debouncer instance behind a shared `Mutex` so
    /// [`Self::add_paths`] can install additional watchers at runtime
    /// (Phase 7C Wave E-2b dynamic `watch_paths`). Dropping the
    /// watcher drops the `Arc`, which — once the last clone is gone
    /// — drops the debouncer, stopping the background thread and
    /// tearing down all installed watchers (see
    /// `notify_debouncer_full::Debouncer`'s `Drop` impl).
    ///
    /// The inner `Option` exists purely so a future shutdown path
    /// could `take()` the debouncer explicitly; today it is always
    /// `Some` after construction.
    debouncer: Arc<Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>>,
}

impl FileChangedWatcher {
    /// Create a new [`FileChangedWatcher`] that watches the given paths
    /// and dispatches debounced [`FileChange`] events to `callback`.
    ///
    /// # Arguments
    ///
    /// - `watch_paths` — `(path, recursive_mode)` pairs to install watchers
    ///   over. Missing or unreadable paths are logged at `debug` level and
    ///   skipped — this mirrors [`ConfigWatcher`] so e.g. a `.envrc` matcher on
    ///   a fresh clone that has not yet been created does not abort the whole
    ///   watcher. An empty list is valid and produces a watcher that simply
    ///   never fires.
    /// - `callback` — user-supplied closure invoked once per debounced
    ///   [`FileChange`] event. Runs on the debouncer's background thread (or on
    ///   a short-lived `std::thread` for delayed deletes), so it must be `Send
    ///   + Sync + 'static`.
    ///
    /// # Errors
    ///
    /// Returns an error if `notify-debouncer-full` cannot start the
    /// debouncer thread (rare — indicates an OS-level notify setup
    /// failure). Individual `watch()` failures are logged and skipped.
    ///
    /// [`ConfigWatcher`]: crate::config_watcher::ConfigWatcher
    pub fn new<F>(watch_paths: Vec<(PathBuf, RecursiveMode)>, callback: F) -> Result<Self>
    where
        F: Fn(FileChange) + Send + Sync + 'static,
    {
        let recent_internal_writes: Arc<Mutex<HashMap<PathBuf, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_unlinks: Arc<Mutex<HashMap<PathBuf, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let last_fired: Arc<Mutex<HashMap<PathBuf, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let state = Arc::new(FileChangedWatcherState {
            callback: Arc::new(callback),
            recent_internal_writes: recent_internal_writes.clone(),
            pending_unlinks,
            last_fired,
        });

        // Clone the state into the debouncer callback. The closure
        // must be `FnMut + Send + 'static`; cloning an `Arc` satisfies
        // both constraints without any interior unsafety.
        let state_for_cb = state.clone();
        let event_handler = move |res: DebounceEventResult| match res {
            Ok(events) => {
                for event in events {
                    // `DebouncedEvent` derefs to `notify::Event`.
                    handle_event(&state_for_cb, &event.event);
                }
            }
            Err(errors) => {
                tracing::warn!(?errors, "file changed watcher errors");
            }
        };

        let mut debouncer = new_debouncer(DEBOUNCE_TIMEOUT, None, event_handler)
            .map_err(|e| anyhow::anyhow!("failed to start file changed watcher: {e}"))?;

        // Install watchers over each requested path. Per-path failures
        // (e.g. path doesn't exist yet) are logged and skipped so the
        // watcher still starts.
        for (path, mode) in watch_paths {
            match debouncer.watch(&path, mode) {
                Ok(()) => {
                    tracing::debug!(
                        path = %path.display(),
                        ?mode,
                        "file changed watcher installed"
                    );
                }
                Err(err) => {
                    tracing::debug!(
                        path = %path.display(),
                        ?mode,
                        error = %err,
                        "file changed watcher skipped path (not watching)"
                    );
                }
            }
        }

        Ok(Self {
            recent_internal_writes,
            debouncer: Arc::new(Mutex::new(Some(debouncer))),
        })
    }

    /// Install additional watchers over the given paths at runtime.
    ///
    /// Used by Phase 7C Wave E-2b dynamic `watch_paths` wiring: when
    /// a `SessionStart` hook returns `watch_paths` in its
    /// [`forge_domain::AggregatedHookResult`], the orchestrator
    /// forwards them to this method so subsequent filesystem changes
    /// under those paths fire `FileChanged` hooks.
    ///
    /// Missing or unreadable paths are logged at `debug` level and
    /// skipped — this mirrors the constructor. Errors are **never**
    /// propagated: the caller has no sensible recovery path for a
    /// runtime watch install failure, and `FileChanged` is an
    /// observability event.
    ///
    /// # Thread safety
    ///
    /// Briefly locks the internal debouncer mutex to call
    /// [`Debouncer::watch`]. Does not block on the debouncer's event
    /// loop — `notify_debouncer_full`'s `watch()` is non-blocking and
    /// returns as soon as the platform-specific watcher has installed
    /// its kernel-level hook.
    pub fn add_paths(&self, watch_paths: Vec<(PathBuf, RecursiveMode)>) {
        let mut guard = self
            .debouncer
            .lock()
            .expect("file changed watcher debouncer mutex poisoned");
        if let Some(debouncer) = guard.as_mut() {
            for (path, mode) in watch_paths {
                match debouncer.watch(&path, mode) {
                    Ok(()) => {
                        tracing::debug!(
                            path = %path.display(),
                            ?mode,
                            "file changed watcher add_paths installed"
                        );
                    }
                    Err(err) => {
                        tracing::debug!(
                            path = %path.display(),
                            ?mode,
                            error = %err,
                            "file changed watcher add_paths skipped path (not watching)"
                        );
                    }
                }
            }
        }
    }

    /// Record that Forge itself is about to write `path`, so any
    /// filesystem event that arrives within the internal-write window
    /// (see [`crate::fs_watcher_core::INTERNAL_WRITE_WINDOW`]) can be
    /// suppressed by the fire loop.
    ///
    /// Both the un-canonicalized and canonicalized forms of `path`
    /// are inserted so that the debouncer callback — which receives
    /// OS-canonical paths — can find the entry regardless of whether
    /// the caller passed in a symlinked path.
    ///
    /// This method is reserved for the future Wave E-2a-cwd work that
    /// will let Forge mutate watched files itself (e.g. when the
    /// `CwdChanged` hook rewrites `.envrc`). Today no fire site calls
    /// it — the Wave E-2a scope is read-only observability.
    pub async fn mark_internal_write(&self, path: impl Into<PathBuf>) {
        let path = path.into();
        let now = Instant::now();
        let canonical = canonicalize_for_lookup(&path);
        let mut guard = self
            .recent_internal_writes
            .lock()
            .expect("recent_internal_writes mutex poisoned");
        guard.insert(path, now);
        guard.insert(canonical, now);
    }

    /// Returns `true` if `path` was marked as an internal write within
    /// the internal-write window. Checks both the as-passed path and
    /// its canonical form so callers can query with either. Used by
    /// tests.
    pub async fn is_internal_write(&self, path: &Path) -> bool {
        is_internal_write_sync(&self.recent_internal_writes, path)
    }
}

/// Fire a [`FileChange`] through `state.callback`, honoring the
/// per-path dispatch cooldown.
///
/// Applies a [`DISPATCH_COOLDOWN`]-per-path cooldown so multi-event
/// batches (e.g. `[Remove, Create, Modify, Modify]` for one atomic
/// save on macOS FSEvents) collapse to one user-visible callback
/// invocation.
///
/// When `bypass_cooldown` is `true`, the cooldown check is skipped but
/// `last_fired` is still updated. This is used exclusively by the
/// delayed-Unlink path: by the time the delayed thread wakes up, a
/// previous `Modify` from the *same* debounce batch may have already
/// updated `last_fired` to only a few hundred milliseconds ago (macOS
/// FSEvents often coalesces `[Remove, Modify]` into one batch on a
/// plain `unlink`). The whole reason we waited `ATOMIC_SAVE_GRACE` was
/// to distinguish a real delete from a collapsed atomic save — the
/// cooldown's "same-batch deduplication" purpose has already been
/// served by waiting, so it must not swallow the delete.
fn fire_change(
    state: &FileChangedWatcherState,
    path: PathBuf,
    event: FileChangeEvent,
    bypass_cooldown: bool,
) {
    // Per-path dispatch cooldown.
    {
        let mut guard = state.last_fired.lock().expect("last_fired mutex poisoned");
        if !bypass_cooldown
            && let Some(last) = guard.get(&path)
            && last.elapsed() < DISPATCH_COOLDOWN
        {
            tracing::debug!(
                path = %path.display(),
                "file changed watcher: coalesced duplicate dispatch within cooldown"
            );
            return;
        }
        guard.insert(path.clone(), Instant::now());
    }

    let change = FileChange { file_path: path, event };
    (state.callback)(change);
}

/// Handle one debounced `notify::Event`. Runs on the debouncer's
/// background thread.
///
/// Per-event behaviour mirrors [`crate::config_watcher`]:
///
/// - `Remove(_)` — stash `(path, now)` in `pending_unlinks` and spawn a
///   short-lived `std::thread` that waits [`ATOMIC_SAVE_GRACE`] and, if the
///   entry is still present (no matching `Create` arrived), removes it and
///   fires a `FileChange` with [`FileChangeEvent::Unlink`]. If a `Create`
///   consumed the entry first, the delayed thread finds it gone and does
///   nothing.
/// - `Create(_)` — if a matching `pending_unlinks` entry exists within the
///   grace window, remove it and fire ONE `FileChange` with
///   [`FileChangeEvent::Change`] (the atomic-save collapse treats the `unlink →
///   add` pair as a single modification). Otherwise fire
///   `FileChangeEvent::Add`.
/// - `Modify(_)` — if the path still exists on disk, fire directly with
///   [`FileChangeEvent::Change`]. If the path has vanished, reclassify as
///   `Remove` and route through the delayed-unlink path; this handles macOS
///   FSEvents, which often reports a plain `fs::remove_file` as a single
///   `Modify` event on the vanished path.
/// - `Access(_)`, `Any`, `Other` — ignored (not mutations).
fn handle_event(state: &Arc<FileChangedWatcherState>, event: &notify::Event) {
    for path in &event.paths {
        // Internal-write suppression applies to every event kind. If
        // a suppression marker is present we consume it (by not
        // clearing it further — the timestamp already causes natural
        // expiry after INTERNAL_WRITE_WINDOW).
        if is_internal_write_sync(&state.recent_internal_writes, path) {
            tracing::debug!(
                path = %path.display(),
                "file changed watcher: suppressed internal write"
            );
            continue;
        }

        match event.kind {
            EventKind::Remove(_) => {
                schedule_delayed_unlink(state, path);
            }
            EventKind::Create(_) => {
                // Check for a pending unlink within the grace window.
                // If present, collapse the pair into a single Change
                // event. Otherwise fire a fresh Add.
                let collapsed = {
                    let mut guard = state
                        .pending_unlinks
                        .lock()
                        .expect("pending_unlinks mutex poisoned");
                    match guard.get(path) {
                        Some(ts) if ts.elapsed() < ATOMIC_SAVE_GRACE => {
                            guard.remove(path);
                            true
                        }
                        Some(_) => {
                            // Stale entry; clean it up so the delayed
                            // thread doesn't fire after us.
                            guard.remove(path);
                            false
                        }
                        None => false,
                    }
                };
                if collapsed {
                    tracing::debug!(
                        path = %path.display(),
                        "file changed watcher: collapsed atomic-save unlink→add"
                    );
                    fire_change(state, path.clone(), FileChangeEvent::Change, false);
                } else {
                    fire_change(state, path.clone(), FileChangeEvent::Add, false);
                }
            }
            EventKind::Modify(_) => {
                // Some platforms (notably macOS FSEvents) report a
                // plain `fs::remove_file` as a `Modify` event on the
                // vanished path rather than a `Remove`. Detect that
                // by probing the filesystem: if the path no longer
                // exists at dispatch time, reclassify as Remove and
                // route through the delayed-unlink path so the
                // atomic-save grace window still has a chance to
                // collapse an incoming Create.
                if !path.exists() {
                    tracing::debug!(
                        path = %path.display(),
                        "file changed watcher: Modify on vanished path, \
                         reclassified as Remove"
                    );
                    schedule_delayed_unlink(state, path);
                } else {
                    fire_change(state, path.clone(), FileChangeEvent::Change, false);
                }
            }
            _ => {
                // Ignore Access, Any, Other — they don't indicate a
                // mutation we care about.
            }
        }
    }
}

/// Stash `(path, now)` in `pending_unlinks` and spawn the delayed
/// Unlink thread. Factored out so both the `Remove` branch and the
/// macOS fallback ("`Modify` on a vanished path") can reuse the exact
/// same atomic-save-grace state machine.
fn schedule_delayed_unlink(state: &Arc<FileChangedWatcherState>, path: &Path) {
    {
        let mut guard = state
            .pending_unlinks
            .lock()
            .expect("pending_unlinks mutex poisoned");
        guard.insert(path.to_path_buf(), Instant::now());
    }

    let state_for_delay = state.clone();
    let path_for_delay = path.to_path_buf();
    std::thread::spawn(move || {
        std::thread::sleep(ATOMIC_SAVE_GRACE);
        // Re-check: if the entry is still present the grace window
        // elapsed without a matching Create, so we fire a delete.
        // If it's gone, a Create already consumed it.
        let still_pending = {
            let mut guard = state_for_delay
                .pending_unlinks
                .lock()
                .expect("pending_unlinks mutex poisoned");
            guard.remove(&path_for_delay).is_some()
        };
        if still_pending {
            fire_change(
                &state_for_delay,
                path_for_delay,
                FileChangeEvent::Unlink,
                true,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use tempfile::TempDir;

    use super::*;

    /// Sleep tick used when polling for async event delivery. Small
    /// enough to keep test latency low but large enough not to burn
    /// the CPU.
    const POLL_TICK: Duration = Duration::from_millis(100);

    /// How long each polling test waits for a file-change event to
    /// show up. Generous because on macOS FSEvents can take over a
    /// second to deliver the first event in a watch session.
    ///
    /// DEBOUNCE_TIMEOUT (1s) + ATOMIC_SAVE_GRACE (1.7s) + 1500ms slack.
    const OBSERVE_TIMEOUT: Duration = Duration::from_millis(4200);

    /// Helper: build a watcher that captures all dispatched events
    /// into a shared `Vec<FileChange>`.
    fn capturing_watcher(dir: &Path) -> (FileChangedWatcher, Arc<Mutex<Vec<FileChange>>>) {
        let captured: Arc<Mutex<Vec<FileChange>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();
        let watcher = FileChangedWatcher::new(
            vec![(dir.to_path_buf(), RecursiveMode::NonRecursive)],
            move |change| {
                captured_clone
                    .lock()
                    .expect("captured mutex poisoned")
                    .push(change);
            },
        )
        .expect("watcher setup");
        (watcher, captured)
    }

    /// Poll `captured` until `predicate` returns true or `OBSERVE_TIMEOUT`
    /// elapses. Returns `true` if the predicate was satisfied, `false`
    /// on timeout. Used in place of a single long sleep so tests finish
    /// as soon as the event arrives.
    fn wait_until<P>(captured: &Arc<Mutex<Vec<FileChange>>>, mut predicate: P) -> bool
    where
        P: FnMut(&[FileChange]) -> bool,
    {
        let deadline = Instant::now() + OBSERVE_TIMEOUT;
        while Instant::now() < deadline {
            {
                let events = captured.lock().expect("captured mutex poisoned");
                if predicate(&events) {
                    return true;
                }
            }
            std::thread::sleep(POLL_TICK);
        }
        false
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_detects_add() {
        let dir = TempDir::new().unwrap();
        let (_watcher, captured) = capturing_watcher(dir.path());

        // Give the watcher a moment to start watching.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let new_file = dir.path().join("added.txt");
        fs::write(&new_file, "hello\n").unwrap();

        let ok = wait_until(&captured, |events| {
            events
                .iter()
                .any(|e| e.event == FileChangeEvent::Add || e.event == FileChangeEvent::Change)
        });
        assert!(
            ok,
            "expected an Add/Change event for newly created file, got: {:?}",
            captured.lock().unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_detects_modify() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("existing.txt");
        fs::write(&target, "initial\n").unwrap();

        let (_watcher, captured) = capturing_watcher(dir.path());

        tokio::time::sleep(Duration::from_millis(200)).await;

        fs::write(&target, "updated\n").unwrap();

        // macOS FSEvents frequently reports in-place overwrites as
        // Create rather than Modify (the truncate-then-write sequence
        // in `fs::write` looks create-ish to the FS layer). Accept any
        // mutation signal (Add or Change) as long as it's on the right
        // file — the test's intent is "the watcher noticed a change",
        // not "the variant was specifically Change".
        let ok = wait_until(&captured, |events| {
            events.iter().any(|e| {
                e.file_path.file_name() == target.file_name()
                    && (e.event == FileChangeEvent::Change || e.event == FileChangeEvent::Add)
            })
        });
        assert!(
            ok,
            "expected a Change/Add event for in-place modification, got: {:?}",
            captured.lock().unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_detects_delete_after_grace() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("doomed.txt");
        fs::write(&target, "initial\n").unwrap();

        let (_watcher, captured) = capturing_watcher(dir.path());

        tokio::time::sleep(Duration::from_millis(200)).await;

        fs::remove_file(&target).unwrap();

        // Need to wait for debounce + grace period + slack before the
        // delete actually fires.
        let ok = wait_until(&captured, |events| {
            events.iter().any(|e| {
                e.file_path.file_name() == target.file_name() && e.event == FileChangeEvent::Unlink
            })
        });
        assert!(
            ok,
            "expected an Unlink event after grace period, got: {:?}",
            captured.lock().unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_collapses_atomic_save() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("saved.txt");
        fs::write(&target, "initial\n").unwrap();

        let (_watcher, captured) = capturing_watcher(dir.path());

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Simulate an atomic save: delete then recreate immediately.
        fs::remove_file(&target).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        fs::write(&target, "updated\n").unwrap();

        // Wait for both the Change dispatch and any would-be Unlink
        // delivery window to expire, so we can assert on the final
        // set.
        tokio::time::sleep(Duration::from_millis(4200)).await;

        let events = captured.lock().unwrap();
        let target_events: Vec<_> = events
            .iter()
            .filter(|e| e.file_path.file_name() == target.file_name())
            .collect();

        // Expect exactly one event for the whole atomic save, and it
        // must NOT be an Unlink — the grace period should have
        // collapsed the pair into a single Change.
        assert_eq!(
            target_events.len(),
            1,
            "expected exactly 1 event for atomic save, got: {:?}",
            target_events
        );
        assert_ne!(
            target_events[0].event,
            FileChangeEvent::Unlink,
            "atomic save should not produce an Unlink event, got: {:?}",
            target_events[0]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_suppresses_internal_write() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("internal.txt");
        fs::write(&target, "initial\n").unwrap();

        let (watcher, captured) = capturing_watcher(dir.path());

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Mark the upcoming write as an internal write, then modify.
        watcher.mark_internal_write(target.clone()).await;
        fs::write(&target, "internal update\n").unwrap();

        // Wait past debounce + slack. We do NOT use wait_until here
        // because we are asserting a *negative* — no event should
        // appear even if we wait longer.
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let events = captured.lock().unwrap();
        let target_events: Vec<_> = events
            .iter()
            .filter(|e| e.file_path.file_name() == target.file_name())
            .collect();
        assert!(
            target_events.is_empty(),
            "expected internal-write suppression to drop events, got: {:?}",
            target_events
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_cooldown_collapses_burst() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("burst.txt");
        fs::write(&target, "initial\n").unwrap();

        let (_watcher, captured) = capturing_watcher(dir.path());

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Three back-to-back writes that should all land inside the
        // same debounce window (and therefore the same cooldown).
        fs::write(&target, "one\n").unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        fs::write(&target, "two\n").unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        fs::write(&target, "three\n").unwrap();

        // Wait for debounce + slack.
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let events = captured.lock().unwrap();
        let target_events: Vec<_> = events
            .iter()
            .filter(|e| e.file_path.file_name() == target.file_name())
            .collect();
        assert_eq!(
            target_events.len(),
            1,
            "expected exactly 1 event for rapid burst, got {}: {:?}",
            target_events.len(),
            target_events
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_skips_missing_paths() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("does_not_exist_yet");
        let present = dir.path();

        let captured: Arc<Mutex<Vec<FileChange>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        // Missing path first, present path second — constructor must
        // skip the missing entry without panicking.
        let watcher = FileChangedWatcher::new(
            vec![
                (missing.clone(), RecursiveMode::NonRecursive),
                (present.to_path_buf(), RecursiveMode::NonRecursive),
            ],
            move |change| {
                captured_clone
                    .lock()
                    .expect("captured mutex poisoned")
                    .push(change);
            },
        )
        .expect("constructor must not fail on missing paths");

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Prove the watcher on the remaining present path still
        // works end-to-end.
        let target = present.join("still_works.txt");
        fs::write(&target, "hello\n").unwrap();

        let ok = wait_until(&captured, |events| {
            events
                .iter()
                .any(|e| e.file_path.file_name() == target.file_name())
        });
        assert!(
            ok,
            "expected a FileChange event on the present path even when one watch path is missing, got: {:?}",
            captured.lock().unwrap()
        );

        // Drop the watcher explicitly so the debouncer thread exits
        // before the tempdir is cleaned up.
        drop(watcher);
    }

    /// Phase 7C Wave E-2b: construct a watcher with an empty initial
    /// path list, then install a runtime watch path via `add_paths`
    /// and prove a fresh write under that path fires a dispatch.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_add_paths_installs_runtime_watcher() {
        let dir = TempDir::new().unwrap();

        let captured: Arc<Mutex<Vec<FileChange>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        // Empty initial watch set — the watcher is live but
        // observing nothing.
        let watcher = FileChangedWatcher::new(Vec::new(), move |change| {
            captured_clone
                .lock()
                .expect("captured mutex poisoned")
                .push(change);
        })
        .expect("watcher must construct with empty paths");

        // Give the debouncer thread a tick to spin up.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Install a runtime watch over the tempdir.
        watcher.add_paths(vec![(
            dir.path().to_path_buf(),
            RecursiveMode::NonRecursive,
        )]);

        // Let the runtime-installed watcher settle.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Prove the runtime-added watch observes new files.
        let target = dir.path().join("dynamic.txt");
        fs::write(&target, "hello from runtime\n").unwrap();

        let ok = wait_until(&captured, |events| {
            events
                .iter()
                .any(|e| e.file_path.file_name() == target.file_name())
        });
        assert!(
            ok,
            "expected a FileChange event for file under runtime-added path, got: {:?}",
            captured.lock().unwrap()
        );

        drop(watcher);
    }

    /// Phase 7C Wave E-2b: calling `add_paths` with a path that does
    /// not exist must neither panic nor error, and must leave the
    /// watcher in a usable state for subsequent valid-path calls.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_changed_watcher_add_paths_tolerates_missing_paths() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("never_created");

        let captured: Arc<Mutex<Vec<FileChange>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        let watcher = FileChangedWatcher::new(Vec::new(), move |change| {
            captured_clone
                .lock()
                .expect("captured mutex poisoned")
                .push(change);
        })
        .expect("watcher must construct with empty paths");

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Install a runtime watch over a path that does not exist —
        // the per-path install fails inside notify, but the error
        // must be swallowed so the rest of the operation proceeds.
        watcher.add_paths(vec![(missing.clone(), RecursiveMode::NonRecursive)]);

        // Follow up with a valid runtime install. If the earlier
        // failure had poisoned any internal state, this call would
        // propagate the failure — instead, it should succeed.
        watcher.add_paths(vec![(
            dir.path().to_path_buf(),
            RecursiveMode::NonRecursive,
        )]);

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Prove the valid path still dispatches events after the
        // missing-path call.
        let target = dir.path().join("post_tolerate.txt");
        fs::write(&target, "still works\n").unwrap();

        let ok = wait_until(&captured, |events| {
            events
                .iter()
                .any(|e| e.file_path.file_name() == target.file_name())
        });
        assert!(
            ok,
            "expected a FileChange event on the valid path after a missing-path add_paths call, got: {:?}",
            captured.lock().unwrap()
        );

        drop(watcher);
    }
}
