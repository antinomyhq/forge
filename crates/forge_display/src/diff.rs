use std::fmt;
use std::path::PathBuf;

use console::{style, Style};
use serde::Serialize;
use similar::{ChangeTag, TextDiff};

use crate::TitleFormat;

struct Line(Option<usize>);

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            None => write!(f, "    "),
            Some(idx) => write!(f, "{:<4}", idx + 1),
        }
    }
}

#[derive(Serialize)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub changes: Vec<DiffChange>,
}

#[derive(Serialize)]
pub struct DiffChange {
    pub tag: String,
    pub old_index: Option<usize>,
    pub new_index: Option<usize>,
    pub content: String,
}

#[derive(Serialize)]
pub struct DiffJson {
    pub op_name: String,
    pub path: String,
    pub unified_diff: String,
    pub hunks: Vec<DiffHunk>,
    pub has_changes: bool,
    pub file_type: String,
}

pub struct DiffFormat;

impl DiffFormat {
    pub fn format(op_name: &str, path: PathBuf, old: &str, new: &str) -> String {
        let diff = TextDiff::from_lines(old, new);
        let ops = diff.grouped_ops(3);

        let mut output = format!(
            "{}\n\n",
            TitleFormat::success(op_name).sub_title(path.display().to_string())
        );

        if ops.is_empty() {
            output.push_str(&format!("{}\n", style("No changes applied").dim()));
            return output;
        }

        for (idx, group) in ops.iter().enumerate() {
            if idx > 0 {
                output.push_str(&format!("{}\n", style("...").dim()));
            }
            for op in group {
                for change in diff.iter_inline_changes(op) {
                    let (sign, s) = match change.tag() {
                        ChangeTag::Delete => ("-", Style::new().blue()),
                        ChangeTag::Insert => ("+", Style::new().yellow()),
                        ChangeTag::Equal => (" ", Style::new().dim()),
                    };

                    output.push_str(&format!(
                        "{}{} |{}",
                        style(Line(change.old_index())).dim(),
                        style(Line(change.new_index())).dim(),
                        s.apply_to(sign),
                    ));

                    for (_, value) in change.iter_strings_lossy() {
                        output.push_str(&format!("{}", s.apply_to(value)));
                    }
                    if change.missing_newline() {
                        output.push('\n');
                    }
                }
            }
        }
        output
    }

    pub fn format_json(op_name: &str, path: PathBuf, old: &str, new: &str) -> DiffJson {
        let diff = TextDiff::from_lines(old, new);
        let ops = diff.grouped_ops(3);

        // Get the unified diff format as well
        let unified_diff = Self::format(op_name, path.clone(), old, new);

        let has_changes = !ops.is_empty();

        // Extract file extension to determine file type for syntax highlighting
        let file_extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

        let file_type = match file_extension {
            "js" => "javascript",
            "ts" => "typescript",
            "tsx" => "tsx",
            "jsx" => "jsx",
            "py" => "python",
            "rs" => "rust",
            "go" => "go",
            "java" => "java",
            "kt" => "kotlin",
            "swift" => "swift",
            "c" | "cpp" | "cc" | "h" | "hpp" => "cpp",
            "cs" => "csharp",
            "php" => "php",
            "rb" => "ruby",
            "scala" => "scala",
            "html" | "htm" => "html",
            "css" => "css",
            "scss" | "sass" => "scss",
            "json" => "json",
            "md" => "markdown",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "xml" => "xml",
            "sh" | "bash" => "bash",
            _ => "plaintext",
        }
        .to_string();

        // Convert ops to hunks
        let mut hunks = Vec::new();

        for group in ops.iter() {
            let mut changes = Vec::new();

            // Find the start and end positions for the hunk
            let mut old_start = usize::MAX;
            let mut old_end = 0;
            let mut new_start = usize::MAX;
            let mut new_end = 0;

            for op in group {
                for change in diff.iter_inline_changes(op) {
                    if let Some(idx) = change.old_index() {
                        old_start = old_start.min(idx);
                        old_end = old_end.max(idx);
                    }
                    if let Some(idx) = change.new_index() {
                        new_start = new_start.min(idx);
                        new_end = new_end.max(idx);
                    }

                    let tag = match change.tag() {
                        ChangeTag::Delete => "delete",
                        ChangeTag::Insert => "insert",
                        ChangeTag::Equal => "equal",
                    }
                    .to_string();

                    let mut content = String::new();
                    for (_, value) in change.iter_strings_lossy() {
                        content.push_str(&value);
                    }

                    changes.push(DiffChange {
                        tag,
                        old_index: change.old_index(),
                        new_index: change.new_index(),
                        content,
                    });
                }
            }

            if old_start == usize::MAX {
                old_start = 0;
            }
            if new_start == usize::MAX {
                new_start = 0;
            }

            hunks.push(DiffHunk {
                old_start,
                old_lines: if old_end >= old_start {
                    old_end - old_start + 1
                } else {
                    0
                },
                new_start,
                new_lines: if new_end >= new_start {
                    new_end - new_start + 1
                } else {
                    0
                },
                changes,
            });
        }

        DiffJson {
            op_name: op_name.to_string(),
            path: path.display().to_string(),
            unified_diff,
            hunks,
            has_changes,
            file_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use console::strip_ansi_codes;
    use insta::assert_snapshot;

    use super::*;

    #[test]
    fn test_color_output() {
        let old = "Hello World\nThis is a test\nThird line\nFourth line";
        let new = "Hello World\nThis is a modified test\nNew line\nFourth line";
        let diff = DiffFormat::format("diff", "example.txt".into(), old, new);
        println!("\nColor Output Test:\n{}", diff);
    }

    #[test]
    fn test_diff_printer_no_differences() {
        let content = "line 1\nline 2\nline 3";
        let diff = DiffFormat::format("diff", "xyz.txt".into(), content, content);
        assert!(diff.contains("No changes applied"));
    }

    #[test]
    fn test_file_source() {
        let old = "line 1\nline 2\nline 3\nline 4\nline 5";
        let new = "line 1\nline 2\nline 3";
        let diff = DiffFormat::format("diff", "xya.txt".into(), old, new);
        let clean_diff = strip_ansi_codes(&diff);
        assert_snapshot!(clean_diff);
    }

    #[test]
    fn test_diff_printer_simple_diff() {
        let old = "line 1\nline 2\nline 3\nline 5\nline 6\nline 7\nline 8\nline 9";
        let new = "line 1\nmodified line\nline 3\nline 5\nline 6\nline 7\nline 8\nline 9";
        let diff = DiffFormat::format("diff", "abc.txt".into(), old, new);
        let clean_diff = strip_ansi_codes(&diff);
        assert_snapshot!(clean_diff);
    }

    #[test]
    fn test_json_diff_format() {
        let old = "line 1\nline 2\nline 3\nline 5\nline 6\nline 7";
        let new = "line 1\nmodified line\nline 3\nline 5\nnew line 6\nline 7";

        let diff_json = DiffFormat::format_json("diff", "test.rs".into(), old, new);

        assert_eq!(diff_json.op_name, "diff");
        assert_eq!(diff_json.path, "test.rs");
        assert_eq!(diff_json.file_type, "rust");
        assert!(diff_json.has_changes);
        assert!(!diff_json.hunks.is_empty());

        // Check the first hunk
        let hunk = &diff_json.hunks[0];
        assert!(hunk
            .changes
            .iter()
            .any(|c| c.tag == "delete" && c.content.contains("line 2")));
        assert!(hunk
            .changes
            .iter()
            .any(|c| c.tag == "insert" && c.content.contains("modified line")));
    }

    #[test]
    fn test_json_diff_no_changes() {
        let content = "line 1\nline 2\nline 3";
        let diff_json = DiffFormat::format_json("diff", "test.js".into(), content, content);

        assert_eq!(diff_json.file_type, "javascript");
        assert!(!diff_json.has_changes);
        assert!(diff_json.hunks.is_empty());
    }
}
