//! Session-scoped hook store for dynamic runtime hook registration.
//!
//! This is the Rust equivalent of claude-code's `sessionHooks.ts`.
//! Hooks registered here persist for the lifetime of a session
//! (or agent sub-session) and are concatenated with static hooks
//! during dispatch.
//!
//! Thread-safe: uses [`RwLock`] for concurrent read access during
//! dispatch and rare write access during registration.

use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{HookEventName, HookMatcher};
use tokio::sync::RwLock;

use crate::hook_runtime::{HookConfigSource, HookMatcherWithSource};

/// Session-scoped hook store for dynamic runtime hook registration.
///
/// Hooks registered here persist for the lifetime of a session
/// (or agent sub-session) and are concatenated with static hooks
/// during dispatch.
///
/// Thread-safe: uses `RwLock` for concurrent read access during
/// dispatch and rare write access during registration.
#[derive(Default, Clone)]
pub struct SessionHookStore {
    inner: Arc<RwLock<HashMap<String, SessionHookBucket>>>,
}

/// Per-session bucket of hooks.
#[derive(Default)]
struct SessionHookBucket {
    hooks: HashMap<HookEventName, Vec<SessionHookEntry>>,
}

/// A single session hook entry.
struct SessionHookEntry {
    matcher: HookMatcher,
    /// Optional root path for `FORGE_PLUGIN_ROOT` env var.
    plugin_root: Option<std::path::PathBuf>,
    /// Plugin name for logging/tracing.
    plugin_name: Option<String>,
}

impl SessionHookStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a hook for a specific session and event.
    ///
    /// Not used in production yet — no code path dynamically registers
    /// hooks at runtime. This entry point exists for future plugin-driven
    /// ephemeral hook registration.
    #[allow(dead_code)] // Extension point: dynamic runtime hook registration.
    pub async fn add_hook(
        &self,
        session_id: &str,
        event: HookEventName,
        matcher: HookMatcher,
        plugin_root: Option<std::path::PathBuf>,
        plugin_name: Option<String>,
    ) {
        let mut guard = self.inner.write().await;
        let bucket = guard.entry(session_id.to_string()).or_default();
        bucket
            .hooks
            .entry(event)
            .or_default()
            .push(SessionHookEntry { matcher, plugin_root, plugin_name });
    }

    /// Get all session hooks for a session and event, converted to
    /// [`HookMatcherWithSource`] for dispatch compatibility.
    pub async fn get_hooks(
        &self,
        session_id: &str,
        event: &HookEventName,
    ) -> Vec<HookMatcherWithSource> {
        let guard = self.inner.read().await;
        let Some(bucket) = guard.get(session_id) else {
            return Vec::new();
        };
        let Some(entries) = bucket.hooks.get(event) else {
            return Vec::new();
        };
        entries
            .iter()
            .map(|e| HookMatcherWithSource {
                matcher: e.matcher.clone(),
                source: HookConfigSource::Session,
                plugin_root: e.plugin_root.clone(),
                plugin_name: e.plugin_name.clone(),
                plugin_options: vec![],
            })
            .collect()
    }

    /// Remove all hooks for a session (cleanup on session end).
    ///
    /// Called by the `SessionEnd` [`EventHandle`] impl on
    /// [`PluginHookHandler`] after all session-end hooks have been
    /// dispatched, preventing unbounded memory growth when multiple
    /// sessions run in the same process.
    pub async fn clear_session(&self, session_id: &str) {
        let mut guard = self.inner.write().await;
        guard.remove(session_id);
    }

    /// Check if any session hooks exist for a given session.
    ///
    /// Intended as a fast-path guard to skip dispatch overhead when no
    /// session hooks are registered. Becomes useful once [`add_hook`]
    /// is wired into production.
    #[allow(dead_code)] // Extension point: fast-path guard for dynamic hooks.
    pub async fn has_hooks(&self, session_id: &str) -> bool {
        let guard = self.inner.read().await;
        guard.get(session_id).is_some_and(|b| !b.hooks.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_domain::{HookCommand, HookEventName, HookMatcher, ShellHookCommand};

    use super::*;
    use crate::hook_runtime::HookConfigSource;

    fn shell_matcher(pattern: &str, command: &str) -> HookMatcher {
        HookMatcher {
            matcher: Some(pattern.to_string()),
            hooks: vec![HookCommand::Command(ShellHookCommand {
                command: command.to_string(),
                condition: None,
                shell: None,
                timeout: None,
                status_message: None,
                once: false,
                async_mode: false,
                async_rewake: false,
            })],
        }
    }

    #[tokio::test]
    async fn test_add_and_get_hooks() {
        let store = SessionHookStore::new();

        store
            .add_hook(
                "sess-1",
                HookEventName::PreToolUse,
                shell_matcher("Bash", "echo hook1"),
                None,
                None,
            )
            .await;

        let hooks = store.get_hooks("sess-1", &HookEventName::PreToolUse).await;
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].source, HookConfigSource::Session);
        assert_eq!(hooks[0].matcher.matcher.as_deref(), Some("Bash"));
    }

    #[tokio::test]
    async fn test_get_hooks_empty_session() {
        let store = SessionHookStore::new();

        let hooks = store
            .get_hooks("nonexistent", &HookEventName::PreToolUse)
            .await;
        assert!(hooks.is_empty());
    }

    #[tokio::test]
    async fn test_get_hooks_empty_event() {
        let store = SessionHookStore::new();

        store
            .add_hook(
                "sess-1",
                HookEventName::PreToolUse,
                shell_matcher("Bash", "echo hook1"),
                None,
                None,
            )
            .await;

        // Different event returns nothing.
        let hooks = store.get_hooks("sess-1", &HookEventName::PostToolUse).await;
        assert!(hooks.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_hooks_same_event() {
        let store = SessionHookStore::new();

        store
            .add_hook(
                "sess-1",
                HookEventName::PreToolUse,
                shell_matcher("Bash", "echo first"),
                None,
                None,
            )
            .await;

        store
            .add_hook(
                "sess-1",
                HookEventName::PreToolUse,
                shell_matcher("Write", "echo second"),
                Some(PathBuf::from("/plugins/my-plugin")),
                Some("my-plugin".to_string()),
            )
            .await;

        let hooks = store.get_hooks("sess-1", &HookEventName::PreToolUse).await;
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].matcher.matcher.as_deref(), Some("Bash"));
        assert!(hooks[0].plugin_root.is_none());

        assert_eq!(hooks[1].matcher.matcher.as_deref(), Some("Write"));
        assert_eq!(
            hooks[1].plugin_root.as_deref(),
            Some(PathBuf::from("/plugins/my-plugin").as_path())
        );
        assert_eq!(hooks[1].plugin_name.as_deref(), Some("my-plugin"));
    }

    #[tokio::test]
    async fn test_clear_session() {
        let store = SessionHookStore::new();

        store
            .add_hook(
                "sess-1",
                HookEventName::PreToolUse,
                shell_matcher("Bash", "echo hook1"),
                None,
                None,
            )
            .await;

        assert!(store.has_hooks("sess-1").await);

        store.clear_session("sess-1").await;

        assert!(!store.has_hooks("sess-1").await);
        let hooks = store.get_hooks("sess-1", &HookEventName::PreToolUse).await;
        assert!(hooks.is_empty());
    }

    #[tokio::test]
    async fn test_clear_session_does_not_affect_other_sessions() {
        let store = SessionHookStore::new();

        store
            .add_hook(
                "sess-1",
                HookEventName::PreToolUse,
                shell_matcher("Bash", "echo hook1"),
                None,
                None,
            )
            .await;

        store
            .add_hook(
                "sess-2",
                HookEventName::PreToolUse,
                shell_matcher("Write", "echo hook2"),
                None,
                None,
            )
            .await;

        store.clear_session("sess-1").await;

        assert!(!store.has_hooks("sess-1").await);
        assert!(store.has_hooks("sess-2").await);

        let hooks = store.get_hooks("sess-2", &HookEventName::PreToolUse).await;
        assert_eq!(hooks.len(), 1);
    }

    #[tokio::test]
    async fn test_has_hooks_empty() {
        let store = SessionHookStore::new();
        assert!(!store.has_hooks("sess-1").await);
    }

    #[tokio::test]
    async fn test_has_hooks_with_hooks() {
        let store = SessionHookStore::new();

        store
            .add_hook(
                "sess-1",
                HookEventName::SessionStart,
                shell_matcher("", "echo start"),
                None,
                None,
            )
            .await;

        assert!(store.has_hooks("sess-1").await);
    }

    #[tokio::test]
    async fn test_hooks_tagged_as_session_source() {
        let store = SessionHookStore::new();

        store
            .add_hook(
                "sess-1",
                HookEventName::PreToolUse,
                shell_matcher("Bash", "echo hook"),
                None,
                None,
            )
            .await;

        let hooks = store.get_hooks("sess-1", &HookEventName::PreToolUse).await;
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].source, HookConfigSource::Session);
    }

    #[tokio::test]
    async fn test_clone_shares_state() {
        let store = SessionHookStore::new();
        let cloned = store.clone();

        store
            .add_hook(
                "sess-1",
                HookEventName::PreToolUse,
                shell_matcher("Bash", "echo shared"),
                None,
                None,
            )
            .await;

        // The clone sees the hook added through the original.
        let hooks = cloned.get_hooks("sess-1", &HookEventName::PreToolUse).await;
        assert_eq!(hooks.len(), 1);
    }
}
