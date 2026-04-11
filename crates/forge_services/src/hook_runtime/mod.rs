//! Hook runtime — the infrastructure for executing hook commands
//! declared in `hooks.json`.
//!
//! This module is split into sub-modules by executor kind plus the
//! dispatch plumbing that wires them together:
//!
//! - [`env`] — builds the `HashMap<String, String>` of `FORGE_*` env vars
//!   injected into every shell hook subprocess.
//! - [`shell`] — the `tokio::process::Command` shell executor.
//! - [`http`] — the HTTP webhook executor (POSTs the input JSON and parses the
//!   response body).
//! - [`llm_common`] -- shared logic for LLM-based hook executors (prompt and
//!   agent hooks), including response schema, `$ARGUMENTS` substitution, and
//!   the common single-shot LLM execution function.
//! - [`prompt`] -- LLM-backed prompt hook executor. Makes a single model call
//!   and parses the `{"ok": bool, "reason"?: string}` response.
//! - [`agent`] -- LLM-backed agent hook executor. Makes a single model call
//!   with a condition-verification system prompt and parses the `{"ok": bool,
//!   "reason"?: string}` response.
//! - [`config_loader`] — merges `hooks.json` from user/project/plugin sources
//!   into a single [`forge_app::hook_runtime::MergedHooksConfig`] used by the
//!   dispatcher.
//! - [`executor`] — the top-level [`forge_app::HookExecutorInfra`] impl that
//!   fans out to the per-kind executors.
//!
//! `HookOutcome` lives in `forge_domain` (not here) so
//! [`forge_domain::AggregatedHookResult::merge`] can consume it without
//! a circular crate dependency. It is re-exported here for convenience
//! so every hook runtime file can `use crate::hook_runtime::HookOutcome;`
//! without pulling in the full `forge_domain::` prefix.

pub mod agent;
pub mod config_loader;
#[cfg(test)]
mod env;
pub mod executor;
pub mod http;
pub(crate) mod llm_common;
pub mod prompt;
pub mod shell;
#[cfg(test)]
pub(crate) mod test_mocks;

pub use config_loader::ForgeHookConfigLoader;
pub use executor::ForgeHookExecutor;
pub use forge_domain::HookOutcome;
