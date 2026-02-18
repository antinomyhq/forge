//! Markdown formatters for tool operations
//!
//! Converts tool operation outputs to markdown format for display in ACP/IDE clients.
//! The XML format is still sent to the LLM, while markdown is used for human-readable display.

use std::path::Path;

use forge_display::DiffFormat;

use crate::utils::format_display_path;
use crate::{FsRemoveOutput, FsWriteOutput, ReadOutput, SearchResult};

/// Formats a file read operation as markdown
///
/// # Arguments
///
/// * `file_path` - Path to the file
/// * `output` - Read operation output
/// * `show_line_numbers` - Whether to show line numbers
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
///
/// # Arguments
///
/// * `file_path` - Path to the file
/// * `output` - Write operation output
/// * `new_content` - The new file content
/// * `cwd` - Current working directory for path formatting
pub fn format_write(
    file_path: &str,
    output: &FsWriteOutput,
    new_content: &str,
    cwd: &Path,
) -> String {
    let display_path = format_display_path(Path::new(file_path), cwd);
    
    if let Some(before) = &output.before {
        let diff_result = DiffFormat::format(before, new_content);
        let diff = console::strip_ansi_codes(diff_result.diff()).to_string();
        
        let mut md = format!(
            "## File Overwritten: `{}`\n\n**Lines:** {}\n\n",
            display_path,
            new_content.lines().count()
        );
        
        md.push_str(&format!("**Changes:** +{} lines, -{} lines\n\n", 
            diff_result.lines_added(), 
            diff_result.lines_removed()
        ));
        
        if !output.errors.is_empty() {
            md.push_str("### ⚠️ Validation Warnings\n\n");
            for error in &output.errors {
                md.push_str(&format!("- {}\n", error));
            }
            md.push('\n');
        }
        
        md.push_str("### Diff\n\n```diff\n");
        md.push_str(&diff);
        md.push_str("\n```");
        
        md
    } else {
        let mut md = format!(
            "## File Created: `{}`\n\n**Lines:** {}\n\n",
            display_path,
            new_content.lines().count()
        );
        
        if !output.errors.is_empty() {
            md.push_str("### ⚠️ Validation Warnings\n\n");
            for error in &output.errors {
                md.push_str(&format!("- {}\n", error));
            }
        }
        
        md
    }
}

/// Formats a file remove operation as markdown
///
/// # Arguments
///
/// * `file_path` - Path to the removed file
/// * `cwd` - Current working directory for path formatting
pub fn format_remove(file_path: &str, _output: &FsRemoveOutput, cwd: &Path) -> String {
    let display_path = format_display_path(Path::new(file_path), cwd);
    format!("## File Removed: `{}`\n\n✓ File successfully deleted", display_path)
}

/// Formats a search operation as markdown
///
/// # Arguments
///
/// * `pattern` - Search pattern
/// * `result` - Search result
/// * `max_lines` - Maximum lines to show
pub fn format_search(pattern: &str, result: &SearchResult, max_lines: usize) -> String {
    let total_matches = result.total_matches();
    let shown = min(result.total_lines(), max_lines);
    
    let mut md = format!(
        "## Search Results: `{}`\n\n**Matches:** {} in {} files\n**Showing:** {} of {} lines\n\n",
        pattern, total_matches, result.files.len(), shown, result.total_lines()
    );
    
    for file in &result.files {
        md.push_str(&format!("### {}\n\n", file.path));
        
        let file_shown = min(file.lines.len(), max_lines);
        for (idx, line) in file.lines.iter().take(file_shown).enumerate() {
            md.push_str(&format!("{}:{}\n", line.line_number, line.content));
        }
        
        if file.lines.len() > file_shown {
            md.push_str(&format!("\n*... {} more lines*\n", file.lines.len() - file_shown));
        }
        md.push('\n');
    }
    
    md
}

/// Formats a search with no results as markdown
pub fn format_search_empty(pattern: &str) -> String {
    format!("## Search Results: `{}`\n\nNo results found.", pattern)
}

/// Formats a codebase search operation as markdown
///
/// # Arguments
///
/// * `query` - Search query
/// * `total_results` - Total number of results
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
///
/// # Arguments
///
/// * `file_path` - Path to the file
/// * `status` - Status description (e.g., "restored", "created", "removed")
pub fn format_undo(file_path: &str, status: &str) -> String {
    format!("## Undo: `{}`\n\n✓ File {}", file_path, status)
}

#[inline]
fn min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_read() {
        let output = ReadOutput {
            content: crate::ReadContent::Text("line1\nline2\nline3".to_string()),
            content_hash: "hash".to_string(),
            start_line: 1,
            end_line: 3,
        };

        let md = format_read("test.txt", &output, false);
        assert!(md.contains("## File: `test.txt`"));
        assert!(md.contains("Lines 1-3 of 3 total"));
        assert!(md.contains("line1\nline2\nline3"));
    }

    #[test]
    fn test_format_write_create() {
        let output = FsWriteOutput {
            before: None,
            content_hash: "hash".to_string(),
            errors: vec![],
        };

        let md = format_write("test.txt", &output, "new content", Path::new("/tmp"));
        assert!(md.contains("## File Created: `test.txt`"));
        assert!(md.contains("**Lines:** 1"));
    }

    #[test]
    fn test_format_write_overwrite() {
        let output = FsWriteOutput {
            before: Some("old\ncontent".to_string()),
            content_hash: "hash".to_string(),
            errors: vec![],
        };

        let md = format_write("test.txt", &output, "new\ncontent\nmore", Path::new("/tmp"));
        assert!(md.contains("## File Overwritten: `test.txt`"));
        assert!(md.contains("**Changes:**"));
        assert!(md.contains("### Diff"));
    }

    #[test]
    fn test_format_remove() {
        let output = FsRemoveOutput {
            content: "removed content".to_string(),
        };

        let md = format_remove("test.txt", &output, Path::new("/tmp"));
        assert!(md.contains("## File Removed: `test.txt`"));
        assert!(md.contains("successfully deleted"));
    }

    #[test]
    fn test_format_search_empty() {
        let md = format_search_empty("pattern");
        assert!(md.contains("## Search Results: `pattern`"));
        assert!(md.contains("No results found"));
    }

    #[test]
    fn test_format_codebase_search_empty() {
        let md = format_codebase_search("query", 0);
        assert!(md.contains("## Semantic Search: `query`"));
        assert!(md.contains("No results found"));
    }

    #[test]
    fn test_format_codebase_search_with_results() {
        let md = format_codebase_search("query", 5);
        assert!(md.contains("## Semantic Search: `query`"));
        assert!(md.contains("**Results:** 5 matches"));
    }

    #[test]
    fn test_format_undo() {
        let md = format_undo("test.txt", "restored");
        assert!(md.contains("## Undo: `test.txt`"));
        assert!(md.contains("File restored"));
    }
}
