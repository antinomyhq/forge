use std::collections::BTreeMap;

use console::style;
use regex::Regex;

/// RipGrepFormatter formats search results in ripgrep-like style.
pub struct GrepFormat(Vec<String>);

impl GrepFormat {
    pub fn new(lines: Vec<String>) -> Self {
        Self(lines)
    }

    /// Format a single line with colorization.
    fn format_line(num: &str, content: &str, regex: &Regex) -> String {
        let mut line = format!("{}{}", style(num).magenta(), style(":").dim());

        match regex.find(content) {
            Some(mat) => {
                line.push_str(&content[..mat.start()]);
                line.push_str(
                    &style(&content[mat.start()..mat.end()])
                        .red()
                        .bold()
                        .to_string(),
                );
                line.push_str(&content[mat.end()..]);
            }
            None => line.push_str(content),
        }

        line.push('\n');
        line
    }

    /// Format search results with colorized output grouped by path.
    pub fn format(&self, regex: &Regex) -> String {
        // Early return for empty results
        if self.0.is_empty() {
            return String::new();
        }

        self.0
            .iter()
            .filter_map(|line| {
                let mut parts = line.splitn(3, ':');
                match (parts.next(), parts.next(), parts.next()) {
                    (Some(path), Some(num), Some(content)) => Some((path, num, content)),
                    _ => None,
                }
            })
            .fold(
                BTreeMap::new(),
                |mut acc: BTreeMap<&str, Vec<(&str, &str)>>, (path, num, content)| {
                    acc.entry(path).or_default().push((num, content));
                    acc
                },
            )
            .into_iter()
            .map(|(path, group)| {
                let file_header = style(path).green().to_string();
                let formatted_lines: String = group
                    .into_iter()
                    .map(|(num, content)| Self::format_line(num, content, regex))
                    .collect();
                format!("{}\n{}", file_header, formatted_lines)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_ripgrep_formatter_single_file() {
        let input = vec!["file.txt:1:first match", "file.txt:2:second match"]
            .into_iter()
            .map(String::from)
            .collect();

        let formatter = GrepFormat(input);
        let result = formatter.format(&Regex::new("match").unwrap());
        let actual = strip_ansi_escapes::strip_str(&result);
        let expected = "file.txt\n1:first match\n2:second match\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_ripgrep_formatter_multiple_files() {
        let input = vec![
            "file1.txt:1:match in file1",
            "file2.txt:1:first match in file2",
            "file2.txt:2:second match in file2",
            "file3.txt:1:match in file3",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let formatter = GrepFormat(input);
        let result = formatter.format(&Regex::new("file").unwrap());
        let actual = strip_ansi_escapes::strip_str(&result);

        let expected = "file1.txt\n1:match in file1\n\nfile2.txt\n1:first match in file2\n2:second match in file2\n\nfile3.txt\n1:match in file3\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_ripgrep_formatter_empty_input() {
        let formatter = GrepFormat(vec![]);
        let result = formatter.format(&Regex::new("file").unwrap());
        assert_eq!(result, "");
    }

    #[test]
    fn test_ripgrep_formatter_malformed_input() {
        let input = vec![
            "file.txt:1:valid match",
            "malformed line without separator",
            "file.txt:2:another valid match",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let formatter = GrepFormat(input);
        let result = formatter.format(&Regex::new("match").unwrap());
        let actual = strip_ansi_escapes::strip_str(&result);

        let expected = "file.txt\n1:valid match\n2:another valid match\n";
        assert_eq!(actual, expected);
    }
}
