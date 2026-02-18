//! Markdown formatters for tool operations
//!
//! Converts tool operation outputs to markdown format for display in ACP/IDE clients.
//! The XML format is still sent to the LLM, while markdown is used for human-readable display.

use std::path::Path;

use forge_display::DiffFormat;
use forge_domain::LineNumbers;

use crate::utils::format_display_path;
use crate::{FsRemoveOutput, FsWriteOutput, ReadOutput, SearchResult};

/// Formats validation warnings as markdown
fn format_warnings<T: std::fmt::Debug>(errors: &[T]) -> String {
    if errors.is_empty() {
        return String::new();
    }
    
    let mut md = String::from("### ⚠️ Validation Warnings\n\n");
    for error in errors {
        md.push_str(&format!("- {:?}\n", error));
    }
    md.push('\n');
    md
}

/// Formats a line with optional line number
fn format_line(line_number: Option<usize>, line: &str) -> String {
    match line_number {
        Some(num) => format!("  Line {}: {}\n", num, line),
        None => format!("  {}\n", line),
    }
}

/// Formats a file read operation as markdown
pub fn format_read(file_path: &str, output: &ReadOutput, show_line_numbers: bool) -> String {
    let content = output.content.file_content();
    let content = if show_line_numbers {
        content.to_numbered_from(output.start_line as usize)
    } else {
        content.to_string()
    };

    format!(
        "## File: `{}`\n\nLines {}-{} of {} total\n\n```\n{}\n```",
        file_path,
        output.start_line,
        output.end_line,
        content.lines().count(),
        content
    )
}

/// Formats a file write operation as markdown
pub fn format_write(
    file_path: &str,
    output: &FsWriteOutput,
    new_content: &str,
    cwd: &Path,
) -> String {
    let display_path = format_display_path(Path::new(file_path), cwd);
    
    let line_count = new_content.lines().count();
    
    if let Some(before) = &output.before {
        let diff_result = DiffFormat::format(before, new_content);
        let diff = console::strip_ansi_codes(diff_result.diff()).to_string();
        
        format!(
            "## File Overwritten: `{}`\n\n**Lines:** {}\n\n**Changes:** +{} lines, -{} lines\n\n{}### Diff\n\n```diff\n{}\n```",
            display_path,
            line_count,
            diff_result.lines_added(),
            diff_result.lines_removed(),
            format_warnings(&output.errors),
            diff
        )
    } else {
        format!(
            "## File Created: `{}`\n\n**Lines:** {}\n\n{}",
            display_path,
            line_count,
            format_warnings(&output.errors)
        )
    }
}

/// Formats a file remove operation as markdown
pub fn format_remove(file_path: &str, _output: &FsRemoveOutput, cwd: &Path) -> String {
    let display_path = format_display_path(Path::new(file_path), cwd);
    format!("## File Removed: `{}`\n\n✓ File successfully deleted", display_path)
}

/// Formats a search operation as markdown
pub fn format_search(pattern: &str, result: &SearchResult, _max_lines: usize) -> String {
    let total_matches = result.matches.len();
    
    let mut md = format!(
        "## Search Results: `{}`\n\n**Files:** {} matches\n\n",
        pattern, total_matches
    );
    
    for file_match in &result.matches {
        md.push_str(&format!("- {}\n", file_match.path));
        
        if let Some(ref match_result) = file_match.result {
            match match_result {
                crate::MatchResult::Found { line_number, line } 
                | crate::MatchResult::ContextMatch { line_number, line, .. } => {
                    md.push_str(&format_line(*line_number, line));
                }
                crate::MatchResult::Count { count } => {
                    md.push_str(&format!("  {} matches\n", count));
                }
                crate::MatchResult::FileMatch => {
                    // Just the file name is enough
                }
                crate::MatchResult::Error(err) => {
                    md.push_str(&format!("  Error: {}\n", err));
                }
            }
        }
    }
    
    md
}

/// Formats a search with no results as markdown
pub fn format_search_empty(pattern: &str) -> String {
    format!("## Search Results: `{}`\n\nNo results found.", pattern)
}

/// Formats a codebase search operation as markdown
pub fn format_codebase_search(query: &str, total_results: usize) -> String {
    if total_results == 0 {
        format!(
            "## Semantic Search: `{}`\n\nNo results found. Try refining your search with more specific terms.",
            query
        )
    } else {
        format!(
            "## Semantic Search: `{}`\n\n**Results:** {} matches found\n\nSee detailed results in the structured output.",
            query, total_results
        )
    }
}

/// Formats an undo operation as markdown
pub fn format_undo(file_path: &str, status: &str) -> String {
    format!("## Undo: `{}`\n\n✓ File {}", file_path, status)
}
