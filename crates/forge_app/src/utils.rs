use std::path::Path;

use forge_domain::Environment;

use crate::{Match, MatchResult};

pub fn display_path(env: &Environment, path: &Path) -> String {
    // Get the current working directory
    let cwd = env.cwd.as_path();

    // Use the shared utility function
    format_display_path(Path::new(path), cwd)
}

/// Formats a path for display, converting absolute paths to relative when
/// possible
///
/// If the path starts with the current working directory, returns a
/// relative path. Otherwise, returns the original absolute path.
///
/// # Arguments
/// * `path` - The path to format
/// * `cwd` - The current working directory path
///
/// # Returns
/// * A formatted path string
fn format_display_path(path: &Path, cwd: &Path) -> String {
    // Try to create a relative path for display if possible
    let display_path = if path.starts_with(cwd) {
        match path.strip_prefix(cwd) {
            Ok(rel_path) => rel_path.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    } else {
        path.display().to_string()
    };

    if display_path.is_empty() {
        ".".to_string()
    } else {
        display_path
    }
}

pub fn format_match(match_: &Match, env: &Environment) -> String {
    match &match_.result {
        Some(MatchResult::Error(err)) => format!("Error reading {}: {}", match_.path, err),
        Some(MatchResult::Found { line_number, line }) => {
            format!(
                "{}:{}:{}",
                display_path(env, Path::new(&match_.path)),
                line_number,
                line
            )
        }
        None => display_path(env, Path::new(&match_.path)),
    }
}
