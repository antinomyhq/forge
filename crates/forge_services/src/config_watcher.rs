//! Configuration file watcher service.
//!
//! The [`ConfigWatcher`] watches Forge's configuration files and
//! directories (`~/.forge/config.toml`, installed plugins, hooks,
//! skills, …) for on-disk changes, debounces the raw filesystem events,
//! and hands the resulting [`ConfigChange`] values to a user-supplied
//! callback so the orchestrator can fire the
//! [`forge_domain::LifecycleEvent::ConfigChange`] plugin hook.
//!
//! # Wave C scope
//!
//! This module ships the real `notify-debouncer-full` event loop with:
//!
//! - a 1-second debounce window (matches Claude Code's
//!   `awaitWriteFinish.stabilityThreshold: 1000`),
//! - 5-second internal-write suppression so Forge's own saves do not round-trip
//!   through the hook system,
//! - a 1.7-second atomic-save grace period so a `unlink → add` pair (Vim,
//!   VSCode, etc.) fires one `Modify`-equivalent event instead of a spurious
//!   delete followed by a create.
//!
//! Wiring the watcher into `ForgeAPI`/`ForgeServices` (and firing the
//! actual `ConfigChange` plugin hook) is handled by Wave C Part 2.
//!
//! # Design notes
//!
//! - **Internal write suppression.** Every time Forge itself writes a watched
//!   config file it calls [`ConfigWatcher::mark_internal_write`] first. When
//!   the filesystem notification finally arrives the debouncer callback
//!   consults the `recent_internal_writes` map and skips the event if the
//!   timestamp is still within the 5-second suppression window. This stops
//!   Forge from firing its own `ConfigChange` hook for saves it made itself.
//! - **Debouncing.** Raw `notify` events are noisy — a single `Save` from a
//!   text editor can produce half a dozen create/modify/rename events.
//!   `notify-debouncer-full` coalesces them into a single event per file per
//!   debounce tick.
//! - **Atomic saves.** Editors like Vim and VSCode save via a `unlink → rename`
//!   sequence. On `Remove` we stash the path in `pending_unlinks` and spawn a
//!   short-lived `std::thread` that waits ~1.7 seconds and, if no `Create` has
//!   consumed the entry in that window, fires a `Remove`-equivalent
//!   `ConfigChange`. If a `Create` arrives first we remove the pending entry
//!   and fire a single `Modify`-equivalent `ConfigChange` for the entire atomic
//!   save.
//! - **Classification.** Plugin hooks filter on the wire string of
//!   [`forge_domain::ConfigSource`] (e.g. `"user_settings"`, `"plugins"`), so
//!   the watcher must know how to translate a raw absolute path back into a
//!   source. [`ConfigWatcher::classify_path`] does that mapping based on
//!   Forge's directory layout.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use forge_domain::ConfigSource;
/// Re-export of `notify::RecursiveMode` so callers don't have to import
/// from `notify_debouncer_full::notify` directly.
pub use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::notify::{self, EventKind, RecommendedWatcher};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::fs_watcher_core::{
    ATOMIC_SAVE_GRACE, DEBOUNCE_TIMEOUT, DISPATCH_COOLDOWN, INTERNAL_WRITE_WINDOW,
    canonicalize_for_lookup, is_internal_write_sync,
};

/// A debounced configuration change detected by [`ConfigWatcher`].
///
/// This is the value handed to the user-supplied callback registered
/// via [`ConfigWatcher::new`]. The orchestrator wraps it in a
/// [`forge_domain::ConfigChangePayload`] and fires the
/// `ConfigChange` lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigChange {
    /// Which config store changed.
    pub source: ConfigSource,
    /// Absolute path of the file (or directory) that changed.
    pub file_path: PathBuf,
}

/// Internal state shared between [`ConfigWatcher`] and the debouncer
/// callback thread. Only holds `Arc`/`Mutex` types so it is trivially
/// `Send + Sync + 'static`, which is required for the
/// `notify-debouncer-full` event handler closure.
struct ConfigWatcherState {
    /// User-supplied callback invoked once per debounced
    /// [`ConfigChange`].
    callback: Arc<dyn Fn(ConfigChange) + Send + Sync>,

    /// Map of paths Forge just wrote → instant the write was recorded.
    /// Consulted on every event so events triggered by Forge's own
    /// saves are suppressed for [`INTERNAL_WRITE_WINDOW`].
    recent_internal_writes: Arc<Mutex<HashMap<PathBuf, Instant>>>,

    /// Map of paths that just saw a `Remove` event → instant the
    /// remove was recorded. Used by the atomic-save grace period to
    /// collapse `unlink → add` pairs into a single `Modify` event.
    pending_unlinks: Arc<Mutex<HashMap<PathBuf, Instant>>>,

    /// Map of paths → instant of the last successful dispatch. Used
    /// by [`fire_change`] to collapse multi-event batches (e.g. the
    /// `[Remove, Create, Modify]` storm macOS emits for an atomic
    /// save) into a single user-visible callback invocation per
    /// [`DISPATCH_COOLDOWN`] window.
    last_fired: Arc<Mutex<HashMap<PathBuf, Instant>>>,
}

/// Service that watches configuration files and directories for
/// changes, debounces the raw events, suppresses events for paths
/// Forge itself just wrote, and collapses atomic-save `unlink → add`
/// pairs into a single modify event.
pub struct ConfigWatcher {
    /// Shared internal-write map. Exposed via
    /// [`mark_internal_write`]/[`is_internal_write`] and handed to the
    /// debouncer callback via [`ConfigWatcherState`].
    recent_internal_writes: Arc<Mutex<HashMap<PathBuf, Instant>>>,

    /// Holds the live debouncer instance. Dropping the watcher drops
    /// the debouncer, which in turn stops the background thread and
    /// drops all installed watchers (see
    /// `notify_debouncer_full::Debouncer`'s `Drop` impl).
    _debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl ConfigWatcher {
    /// Create a new [`ConfigWatcher`] that watches the given paths and
    /// dispatches debounced [`ConfigChange`] events to `callback`.
    ///
    /// # Arguments
    ///
    /// - `watch_paths` — `(path, recursive_mode)` pairs to install watchers
    ///   over. Missing or unreadable paths are logged at `debug` level and
    ///   skipped so e.g. a non-existent `~/forge/plugins/` directory on first
    ///   startup does not abort the whole watcher. An empty list is valid and
    ///   produces a watcher that simply never fires.
    /// - `callback` — user-supplied closure invoked once per debounced
    ///   [`ConfigChange`] event. Runs on the debouncer's background thread (or
    ///   on a short-lived `std::thread` for delayed deletes), so it must be
    ///   `Send + Sync + 'static`.
    ///
    /// # Errors
    ///
    /// Returns an error if `notify-debouncer-full` cannot start the
    /// debouncer thread (rare — indicates an OS-level notify setup
    /// failure). Individual `watch()` failures are logged and skipped.
    pub fn new<F>(watch_paths: Vec<(PathBuf, RecursiveMode)>, callback: F) -> Result<Self>
    where
        F: Fn(ConfigChange) + Send + Sync + 'static,
    {
        let recent_internal_writes: Arc<Mutex<HashMap<PathBuf, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_unlinks: Arc<Mutex<HashMap<PathBuf, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let last_fired: Arc<Mutex<HashMap<PathBuf, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let state = Arc::new(ConfigWatcherState {
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
                tracing::warn!(?errors, "config watcher errors");
            }
        };

        let mut debouncer = new_debouncer(DEBOUNCE_TIMEOUT, None, event_handler)
            .map_err(|e| anyhow::anyhow!("failed to start config watcher: {e}"))?;

        // Install watchers over each requested path. Per-path failures
        // (e.g. directory doesn't exist yet) are logged and skipped so
        // the watcher still starts.
        for (path, mode) in watch_paths {
            match debouncer.watch(&path, mode) {
                Ok(()) => {
                    tracing::debug!(path = %path.display(), ?mode, "config watcher installed");
                }
                Err(err) => {
                    tracing::debug!(
                        path = %path.display(),
                        ?mode,
                        error = %err,
                        "config watcher skipped path (not watching)"
                    );
                }
            }
        }

        Ok(Self { recent_internal_writes, _debouncer: Some(debouncer) })
    }

    /// Record that Forge itself is about to write `path`, so any
    /// filesystem event that arrives within [`INTERNAL_WRITE_WINDOW`]
    /// can be suppressed by the fire loop.
    ///
    /// Both the un-canonicalized and canonicalized forms of `path`
    /// are inserted so that the debouncer callback — which receives
    /// OS-canonical paths — can find the entry regardless of whether
    /// the caller passed in a symlinked path.
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
    /// the last [`INTERNAL_WRITE_WINDOW`]. Checks both the as-passed
    /// path and its canonical form so callers can query with either.
    pub async fn is_internal_write(&self, path: &Path) -> bool {
        let canonical = canonicalize_for_lookup(path);
        let guard = self
            .recent_internal_writes
            .lock()
            .expect("recent_internal_writes mutex poisoned");
        let hit = |p: &Path| {
            guard
                .get(p)
                .map(|ts| ts.elapsed() < INTERNAL_WRITE_WINDOW)
                .unwrap_or(false)
        };
        hit(path) || hit(&canonical)
    }

    /// Classify a filesystem path into a [`ConfigSource`] based on
    /// Forge's directory layout.
    ///
    /// This is a pure function so callers can use it without having to
    /// spin up a full [`ConfigWatcher`]. The mapping rules:
    ///
    /// | Path shape                         | Source           |
    /// |------------------------------------|------------------|
    /// | `…/.forge/local.toml`              | `LocalSettings`  |
    /// | `…/forge/.forge.toml`              | `UserSettings`   |
    /// | `…/.forge/config.toml`             | `ProjectSettings`|
    /// | `…hooks.json`                      | `Hooks`          |
    /// | `…/plugins/…`                      | `Plugins`        |
    /// | `…/skills/…`                       | `Skills`         |
    /// | anything else                      | `None`           |
    ///
    /// Policy settings are intentionally not classified here — the
    /// policy path is OS-specific and must be resolved by the caller
    /// before mapping.
    pub fn classify_path(path: &Path) -> Option<ConfigSource> {
        let s = path.to_string_lossy();
        if s.contains("/.forge/local.toml") || s.ends_with("local.toml") {
            Some(ConfigSource::LocalSettings)
        } else if s.contains("/forge/.forge.toml") || s.ends_with(".forge.toml") {
            Some(ConfigSource::UserSettings)
        } else if s.contains("/.forge/config.toml") {
            Some(ConfigSource::ProjectSettings)
        } else if s.contains("hooks.json") {
            Some(ConfigSource::Hooks)
        } else if s.contains("/plugins/") {
            Some(ConfigSource::Plugins)
        } else if s.contains("/skills/") {
            Some(ConfigSource::Skills)
        } else {
            None
        }
    }
}

/// Fire a [`ConfigChange`] through `state.callback` if `path` maps to a
/// known [`ConfigSource`].
///
/// Applies a [`DISPATCH_COOLDOWN`]-per-path cooldown so multi-event
/// batches (e.g. `[Remove, Create, Modify, Modify]` for one atomic
/// save on macOS FSEvents) collapse to one user-visible callback
/// invocation. Paths that do not classify (e.g. random files inside
/// a watched directory) are logged at debug level and dropped.
fn fire_change(state: &ConfigWatcherState, path: PathBuf) {
    let Some(source) = ConfigWatcher::classify_path(&path) else {
        tracing::debug!(path = %path.display(), "config watcher: path did not classify");
        return;
    };

    // Per-path dispatch cooldown.
    {
        let mut guard = state.last_fired.lock().expect("last_fired mutex poisoned");
        if let Some(last) = guard.get(&path)
            && last.elapsed() < DISPATCH_COOLDOWN
        {
            tracing::debug!(
                path = %path.display(),
                "config watcher: coalesced duplicate dispatch within cooldown"
            );
            return;
        }
        guard.insert(path.clone(), Instant::now());
    }

    let change = ConfigChange { source, file_path: path };
    (state.callback)(change);
}

/// Handle one debounced `notify::Event`. Runs on the debouncer's
/// background thread.
///
/// Per-event behaviour:
///
/// - `Remove(_)` — stash `(path, now)` in `pending_unlinks` and spawn a
///   short-lived `std::thread` that waits [`ATOMIC_SAVE_GRACE`] and, if the
///   entry is still present (no matching `Create` arrived), removes it and
///   fires a `ConfigChange`. If a `Create` consumed the entry first, the
///   delayed thread finds it gone and does nothing.
/// - `Create(_)` — if a matching `pending_unlinks` entry exists within the
///   grace window, remove it and fire ONE `ConfigChange` (the atomic-save
///   collapse). Otherwise fire a fresh `ConfigChange`.
/// - `Modify(_)` — fire directly (after internal-write check).
/// - Anything else — ignored.
fn handle_event(state: &Arc<ConfigWatcherState>, event: &notify::Event) {
    for path in &event.paths {
        // Internal-write suppression applies to every event kind.
        if is_internal_write_sync(&state.recent_internal_writes, path) {
            tracing::debug!(
                path = %path.display(),
                "config watcher: suppressed internal write"
            );
            continue;
        }

        match event.kind {
            EventKind::Remove(_) => {
                // Stash the unlink and spawn a delayed fire. Cloning
                // the Arc into the thread is cheap and keeps the
                // closure `Send + 'static`.
                {
                    let mut guard = state
                        .pending_unlinks
                        .lock()
                        .expect("pending_unlinks mutex poisoned");
                    guard.insert(path.clone(), Instant::now());
                }

                let state_for_delay = state.clone();
                let path_for_delay = path.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(ATOMIC_SAVE_GRACE);
                    // Re-check: if the entry is still present the
                    // grace window elapsed without a matching Create,
                    // so we fire a delete. If it's gone, a Create
                    // already consumed it.
                    let still_pending = {
                        let mut guard = state_for_delay
                            .pending_unlinks
                            .lock()
                            .expect("pending_unlinks mutex poisoned");
                        guard.remove(&path_for_delay).is_some()
                    };
                    if still_pending {
                        fire_change(&state_for_delay, path_for_delay);
                    }
                });
            }
            EventKind::Create(_) => {
                // Check for a pending unlink within the grace window.
                // Whether or not one is present we still fire exactly
                // one ConfigChange — the difference is just that a
                // collapsed atomic save does not additionally fire the
                // delayed delete.
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
                        "config watcher: collapsed atomic-save unlink→add"
                    );
                }
                fire_change(state, path.clone());
            }
            EventKind::Modify(_) => {
                fire_change(state, path.clone());
            }
            _ => {
                // Ignore Access, Any, Other — they don't indicate a
                // config change we care about.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    // ---- classify_path ----

    #[test]
    fn test_classify_path_user_settings() {
        let path = PathBuf::from("/home/alice/forge/.forge.toml");
        let actual = ConfigWatcher::classify_path(&path);
        assert_eq!(actual, Some(ConfigSource::UserSettings));
    }

    #[test]
    fn test_classify_path_project_settings() {
        let path = PathBuf::from("/work/myproj/.forge/config.toml");
        let actual = ConfigWatcher::classify_path(&path);
        assert_eq!(actual, Some(ConfigSource::ProjectSettings));
    }

    #[test]
    fn test_classify_path_plugin_directory() {
        let path = PathBuf::from("/home/alice/forge/plugins/acme/plugin.toml");
        let actual = ConfigWatcher::classify_path(&path);
        assert_eq!(actual, Some(ConfigSource::Plugins));
    }

    #[test]
    fn test_classify_path_hooks_json() {
        let path = PathBuf::from("/home/alice/forge/hooks.json");
        let actual = ConfigWatcher::classify_path(&path);
        assert_eq!(actual, Some(ConfigSource::Hooks));
    }

    #[test]
    fn test_classify_path_unknown_returns_none() {
        let path = PathBuf::from("/tmp/some/random/file.txt");
        let actual = ConfigWatcher::classify_path(&path);
        assert_eq!(actual, None);
    }

    // ---- internal-write suppression ----

    /// Helper that constructs a minimal `ConfigWatcher` with an empty
    /// watch set and a no-op callback so tests can exercise the
    /// internal-write API without installing any real filesystem
    /// watchers.
    fn test_watcher() -> ConfigWatcher {
        ConfigWatcher::new(vec![], |_change: ConfigChange| {})
            .expect("ctor is infallible for empty watch_paths")
    }

    #[tokio::test]
    async fn test_mark_internal_write_then_is_internal_write_true() {
        let watcher = test_watcher();
        let path = PathBuf::from("/home/alice/forge/config.toml");

        watcher.mark_internal_write(path.clone()).await;

        assert!(watcher.is_internal_write(&path).await);
    }

    #[tokio::test]
    async fn test_is_internal_write_false_after_expiry() {
        // We seed the map directly with an Instant in the past so we
        // don't depend on wall-clock sleeping.
        let watcher = test_watcher();
        let path = PathBuf::from("/home/alice/forge/config.toml");

        {
            let mut guard = watcher
                .recent_internal_writes
                .lock()
                .expect("recent_internal_writes mutex poisoned");
            // 10 seconds ago — comfortably outside the 5-second window.
            guard.insert(path.clone(), Instant::now() - Duration::from_secs(10));
        }

        assert!(!watcher.is_internal_write(&path).await);
    }

    #[tokio::test]
    async fn test_is_internal_write_false_for_unknown_path() {
        let watcher = test_watcher();
        let path = PathBuf::from("/never/marked.toml");

        assert!(!watcher.is_internal_write(&path).await);
    }

    // ---- real debouncer wiring ----
    //
    // These tests exercise the actual `notify-debouncer-full` event
    // loop against a real temp directory. They are inherently timing
    // sensitive (the debounce window is 1s and the grace period is
    // 1.7s) so each test waits several seconds for events to settle.

    /// Helper: build a watcher that captures all dispatched events
    /// into a shared `Vec<ConfigChange>`.
    fn capturing_watcher(dir: &Path) -> (ConfigWatcher, Arc<Mutex<Vec<ConfigChange>>>) {
        let captured: Arc<Mutex<Vec<ConfigChange>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();
        let watcher = ConfigWatcher::new(
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_real_file_write_fires_config_change() {
        use std::fs;

        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        // classify_path recognises `.forge.toml` via its trailing
        // suffix, so the enclosing directory name does not matter.
        let forge_dir = dir.path().join("forge");
        fs::create_dir(&forge_dir).unwrap();
        let config_path = forge_dir.join(".forge.toml");
        fs::write(&config_path, "initial = 1\n").unwrap();

        let (_watcher, captured) = capturing_watcher(&forge_dir);

        // Give the watcher a moment to start watching.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Modify the file.
        fs::write(&config_path, "updated = 2\n").unwrap();

        // Wait for the debouncer to fire (1s debounce + slack).
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let events = captured.lock().unwrap();
        assert!(
            !events.is_empty(),
            "Expected at least one ConfigChange event for .forge.toml modification"
        );
        let matched = events.iter().any(|e| {
            e.source == ConfigSource::UserSettings
                && e.file_path
                    .file_name()
                    .map(|n| n == ".forge.toml")
                    .unwrap_or(false)
        });
        assert!(
            matched,
            "Expected a UserSettings event for .forge.toml, got: {:?}",
            *events
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_internal_write_suppression_end_to_end() {
        use std::fs;

        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let forge_dir = dir.path().join("forge");
        fs::create_dir(&forge_dir).unwrap();
        let config_path = forge_dir.join(".forge.toml");
        fs::write(&config_path, "initial = 1\n").unwrap();

        let (watcher, captured) = capturing_watcher(&forge_dir);

        // Give the watcher a moment to start watching.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Mark the upcoming write as an internal write, then modify.
        watcher.mark_internal_write(config_path.clone()).await;
        fs::write(&config_path, "updated = 2\n").unwrap();

        // Wait for the debouncer to fire.
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let events = captured.lock().unwrap();
        let forge_toml_events: Vec<_> = events
            .iter()
            .filter(|e| {
                e.file_path
                    .file_name()
                    .map(|n| n == ".forge.toml")
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            forge_toml_events.is_empty(),
            "Expected internal-write suppression to drop events, got: {:?}",
            forge_toml_events
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_atomic_save_fires_once() {
        use std::fs;

        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let forge_dir = dir.path().join("forge");
        fs::create_dir(&forge_dir).unwrap();
        let config_path = forge_dir.join(".forge.toml");
        fs::write(&config_path, "initial = 1\n").unwrap();

        let (_watcher, captured) = capturing_watcher(&forge_dir);

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Simulate an atomic save: delete then recreate immediately.
        fs::remove_file(&config_path).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        fs::write(&config_path, "updated = 2\n").unwrap();

        // Wait for the 1s debounce + 1.7s grace period + slack.
        tokio::time::sleep(Duration::from_millis(3500)).await;

        let events = captured.lock().unwrap();
        let forge_toml_events: Vec<_> = events
            .iter()
            .filter(|e| {
                e.file_path
                    .file_name()
                    .map(|n| n == ".forge.toml")
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(
            forge_toml_events.len(),
            1,
            "Expected exactly 1 event for atomic save, got {}: {:?}",
            forge_toml_events.len(),
            forge_toml_events
        );
    }
}
