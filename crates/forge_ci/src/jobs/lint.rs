//! Shared lint commands for CI workflows

/// Build a cargo command from parts
fn cargo_cmd(parts: &[&str]) -> String {
    parts.join(" ")
}

/// Base parts for fmt commands
fn fmt_base() -> Vec<&'static str> {
    vec!["cargo", "+nightly", "fmt", "--all"]
}

/// Base parts for clippy commands
fn clippy_base() -> Vec<&'static str> {
    vec![
        "cargo",
        "+nightly",
        "clippy",
        "--all-features",
        "--all-targets",
        "--workspace",
    ]
}

/// Build a cargo fmt command
pub fn fmt_cmd(fix: bool) -> String {
    let mut parts = fmt_base();
    if !fix {
        parts.push("--check");
    }
    cargo_cmd(&parts)
}

/// Build a cargo clippy command
pub fn clippy_cmd(fix: bool) -> String {
    let mut parts = clippy_base();

    if fix {
        parts.extend(["--fix", "--allow-dirty"]);
    }

    parts.extend(["--", "-D", "warnings"]);

    cargo_cmd(&parts)
}
