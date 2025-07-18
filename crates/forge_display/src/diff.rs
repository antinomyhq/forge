use std::fmt;

use console::{Style, style};
use similar::{ChangeTag, TextDiff};

struct Line(Option<usize>);

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            None => write!(f, "    "),
            Some(idx) => write!(f, "{:<4}", idx + 1),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiffResult {
    result: String,
    lines_added: u64,
    lines_removed: u64,
}

impl DiffResult {
    pub fn diff(&self) -> &str {
        &self.result
    }

    pub fn lines_added(&self) -> u64 {
        self.lines_added
    }

    pub fn lines_removed(&self) -> u64 {
        self.lines_removed
    }
}

pub struct DiffFormat;

impl DiffFormat {
    pub fn format(old: &str, new: &str) -> DiffResult {
        let diff = TextDiff::from_lines(old, new);
        let ops = diff.grouped_ops(3);
        let mut output = String::new();

        let mut lines_added = 0;
        let mut lines_removed = 0;

        if ops.is_empty() {
            output.push_str(&format!("{}\n", style("No changes applied").dim()));

            return DiffResult { result: output, lines_added, lines_removed };
        }

        for (idx, group) in ops.iter().enumerate() {
            if idx > 0 {
                output.push_str(&format!("{}\n", style("...").dim()));
            }
            for op in group {
                for change in diff.iter_inline_changes(op) {
                    let (sign, s) = match change.tag() {
                        ChangeTag::Delete => {
                            lines_removed += 1;
                            ("-", Style::new().red())
                        }
                        ChangeTag::Insert => {
                            lines_added += 1;
                            ("+", Style::new().yellow())
                        }
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

        DiffResult { result: output, lines_added, lines_removed }
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
        let diff = DiffFormat::format(old, new);
        let diff_str = diff.diff();
        assert_eq!(diff.lines_added(), 2);
        assert_eq!(diff.lines_removed(), 2);
        eprintln!("\nColor Output Test:\n{diff_str}");
    }

    #[test]
    fn test_diff_printer_no_differences() {
        let content = "line 1\nline 2\nline 3";
        let diff = DiffFormat::format(content, content);
        assert_eq!(diff.lines_added(), 0);
        assert_eq!(diff.lines_removed(), 0);
        assert!(diff.diff().contains("No changes applied"));
    }

    #[test]
    fn test_file_source() {
        let old = "line 1\nline 2\nline 3\nline 4\nline 5";
        let new = "line 1\nline 2\nline 3";
        let diff = DiffFormat::format(old, new);
        let clean_diff = strip_ansi_codes(&diff.diff());
        assert_eq!(diff.lines_added(), 1);
        assert_eq!(diff.lines_removed(), 3);
        assert_snapshot!(clean_diff);
    }

    #[test]
    fn test_diff_printer_simple_diff() {
        let old = "line 1\nline 2\nline 3\nline 5\nline 6\nline 7\nline 8\nline 9";
        let new = "line 1\nmodified line\nline 3\nline 5\nline 6\nline 7\nline 8\nline 9";
        let diff = DiffFormat::format(old, new);
        let clean_diff = strip_ansi_codes(&diff.diff());
        assert_eq!(diff.lines_added(), 1);
        assert_eq!(diff.lines_removed(), 1);
        assert_snapshot!(clean_diff);
    }
}
