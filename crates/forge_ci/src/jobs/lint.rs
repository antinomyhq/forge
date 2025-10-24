//! Shared lint commands for CI workflows

/// Cargo fmt command for checking formatting
pub const FMT_CHECK_CMD: &str = "cargo +nightly fmt --all --check";

/// Cargo fmt command for fixing formatting
pub const FMT_FIX_CMD: &str = "cargo +nightly fmt --all";

/// Cargo clippy command for checking lints
pub const CLIPPY_CHECK_CMD: &str = "cargo +nightly clippy --all-features --all-targets --workspace";

/// Cargo clippy command for fixing lints
pub const CLIPPY_FIX_CMD: &str =
    "cargo +nightly clippy --fix --allow-dirty --all-features --workspace -- -D warnings";
