use std::path::Path;

use crate::{Match, MatchResult};

/// Formats a path for display, converting absolute to relative when possible.
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

/// Truncates a key for display (shows first 13 and last 4 chars if longer than
/// 20).
pub fn truncate_key(key: &str) -> String {
    if key.len() <= 20 {
        key.to_string()
    } else {
        format!("{}...{}", &key[..=12], &key[key.len() - 4..])
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

/// Computes SHA-256 hash of the given content.
pub fn compute_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}
