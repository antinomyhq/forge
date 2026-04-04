//! Shared shell integration utilities.
//!
//! This module provides shell-agnostic types and functions used by
//! shell-specific modules (zsh, powershell, etc.) to avoid duplicating
//! common logic like prompt data fetching, ANSI styling, and profile setup.

pub mod prompt;
pub mod setup;
pub mod style;

/// Normalizes shell script content for cross-platform compatibility.
///
/// Strips carriage returns (`\r`) that appear when `include_str!` or
/// `include_dir!` embed files on Windows (where `git core.autocrlf=true`
/// converts LF to CRLF on checkout). Most shells cannot parse `\r` in scripts.
pub(crate) fn normalize_script(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}
