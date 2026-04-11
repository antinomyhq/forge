//! Public types and traits shared across the hook runtime.
//!
//! These types live in `forge_app` so both the upstream dispatcher
//! (`forge_app::hooks::plugin::PluginHookHandler`) and the downstream
//! implementations in `forge_services::hook_runtime` can reference the
//! same shapes without creating a circular crate dependency.
//!
//! The concrete loader (`ForgeHookConfigLoader`) and executor
//! (`ForgeHookExecutor`) live in `forge_services::hook_runtime`. This
//! module only defines the trait surfaces and the merged-config data
//! types that the dispatcher consumes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use forge_domain::{HookEventName, HookMatcher};

/// Where a [`HookMatcher`] came from. Used so the shell executor can
/// populate `FORGE_PLUGIN_ROOT` / `CLAUDE_PLUGIN_ROOT` correctly for
/// plugin-sourced hooks, and so logs can distinguish user vs plugin
/// failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookConfigSource {
    /// `~/forge/hooks.json`.
    UserGlobal,
    /// `./.forge/hooks.json`.
    Project,
    /// A plugin's `manifest.hooks` — inline, path, or array.
    Plugin,
    /// Enterprise-managed hooks loaded from a managed hooks path.
    Managed,
    /// Runtime-registered hooks scoped to a session's lifetime.
    Session,
}

/// A [`HookMatcher`] tagged with its source so the dispatcher can
/// build the right environment variables and logging context.
#[derive(Debug, Clone)]
pub struct HookMatcherWithSource {
    /// The underlying matcher parsed from `hooks.json`.
    pub matcher: HookMatcher,
    /// Which file (user/project/plugin) contributed this matcher.
    pub source: HookConfigSource,
    /// Plugin root directory, populated when `source == Plugin`.
    /// Exposed to shell hooks as `FORGE_PLUGIN_ROOT`.
    pub plugin_root: Option<PathBuf>,
    /// Plugin name, populated when `source == Plugin`. Used by the
    /// dispatcher's `once_fired` map to give each plugin-scoped hook
    /// a unique identity.
    pub plugin_name: Option<String>,
    /// User-configured plugin options from ForgeConfig.plugins[name].options.
    /// Passed to shell hooks as FORGE_PLUGIN_OPTION_<KEY> env vars.
    pub plugin_options: Vec<(String, String)>,
}

/// Result of merging `hooks.json` from every configured source.
///
/// Keyed by event name with a flat vector of matchers per event. The
/// dispatcher iterates these in insertion order: user → project → plugins.
#[derive(Debug, Clone, Default)]
pub struct MergedHooksConfig {
    /// Per-event list of matchers, tagged with their source.
    pub entries: BTreeMap<HookEventName, Vec<HookMatcherWithSource>>,
}

impl MergedHooksConfig {
    /// Returns `true` when no matchers were loaded from any source.
    pub fn is_empty(&self) -> bool {
        self.entries.values().all(|v| v.is_empty())
    }

    /// Returns `true` when at least one matcher is loaded from any source.
    ///
    /// This is the semantic inverse of [`is_empty`](Self::is_empty) and
    /// exists so callers can express fast-path guards naturally:
    ///
    /// ```ignore
    /// if !merged.has_hooks() { return Ok(default); }
    /// ```
    pub fn has_hooks(&self) -> bool {
        !self.is_empty()
    }

    /// Total number of matchers across every event. Useful for tests
    /// and logging.
    pub fn total_matchers(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }
}

/// Trait for loading (and caching) the [`MergedHooksConfig`].
///
/// The concrete implementation lives in
/// `forge_services::hook_runtime::config_loader::ForgeHookConfigLoader`
/// and is wired into [`crate::Services`] via the associated type.
#[async_trait::async_trait]
pub trait HookConfigLoaderService: Send + Sync {
    /// Load the merged hooks configuration from disk, returning a cached
    /// copy on subsequent calls until [`invalidate`](Self::invalidate)
    /// is invoked.
    async fn load(&self) -> anyhow::Result<Arc<MergedHooksConfig>>;

    /// Drop any cached merged config so the next [`load`](Self::load)
    /// re-reads all sources from disk.
    async fn invalidate(&self) -> anyhow::Result<()>;
}
