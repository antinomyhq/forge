use std::fmt;
use std::path::PathBuf;

use console::{style, Style};
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
}
