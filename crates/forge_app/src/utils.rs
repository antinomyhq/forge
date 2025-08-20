use std::path::Path;

use crate::{Match, MatchResult};

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
pub fn format_display_path(path: &Path, cwd: &Path) -> String {
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

pub fn format_match(matched: &Match, base_dir: &Path) -> String {
    match &matched.result {
        Some(MatchResult::Error(err)) => format!("Error reading {}: {}", matched.path, err),
        Some(MatchResult::Found { line_number, line }) => {
            format!(
                "{}:{}:{}",
                format_display_path(Path::new(&matched.path), base_dir),
                line_number,
                line
            )
        }
        None => format_display_path(Path::new(&matched.path), base_dir),
    }
}
