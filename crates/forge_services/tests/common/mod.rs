//! Shared helpers for Claude Code plugin compatibility integration tests.
//!
//! Provides path helpers for the Wave G-1 fixture plugins checked in under
//! `crates/forge_services/tests/fixtures/plugins/`. Downstream test files
//! (e.g. `plugin_fixtures_test.rs`, and future Wave G-2 hook execution
//! tests) use these helpers to locate fixtures in a way that's independent
//! of the process's working directory at test run time.
//!
//! # Usage
//!
//! Each integration test file in `tests/` that needs these helpers should
//! declare the module at its top:
//!
//! ```ignore
//! mod common;
//! use common::{fixture_plugins_dir, fixture_plugin_path, list_fixture_plugin_names};
//! ```
//!
//! Rust's integration-test runner compiles each `tests/*.rs` file as its
//! own crate, so `common/mod.rs` is shared by convention — it is only
//! recompiled per test file. The `#[allow(dead_code)]` on each helper
//! prevents warnings in files that only use a subset of the API.

use std::path::PathBuf;

/// Ordered list of all Wave G-1 fixture plugin directory names.
///
/// Kept as an associated constant so downstream tests can iterate it and
/// assert against a stable set. The names match the `name` field in each
/// plugin's `.claude-plugin/plugin.json`.
pub const FIXTURE_PLUGIN_NAMES: &[&str] = &[
    "agent-provider",
    "bash-logger",
    "command-provider",
    "config-watcher",
    "dangerous-guard",
    "full-stack",
    "prettier-format",
    "skill-provider",
];

/// Returns the absolute path to `tests/fixtures/plugins/`.
///
/// Uses `CARGO_MANIFEST_DIR` so the resolved path does not depend on the
/// working directory of the test runner. This is the canonical root that
/// Wave G-1 / G-2 tests point at when exercising plugin discovery.
#[allow(dead_code)]
pub fn fixture_plugins_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("plugins")
}

/// Returns the absolute path to a specific fixture plugin directory.
#[allow(dead_code)]
pub fn fixture_plugin_path(name: &str) -> PathBuf {
    fixture_plugins_dir().join(name)
}

/// Returns all 8 fixture plugin names in a stable order.
#[allow(dead_code)]
pub fn list_fixture_plugin_names() -> Vec<&'static str> {
    FIXTURE_PLUGIN_NAMES.to_vec()
}
