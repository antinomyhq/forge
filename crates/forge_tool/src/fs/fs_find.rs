use std::collections::HashSet;
use std::path::Path;

use console::style;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use forge_walker::Walker;
use regex::Regex;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::utils::assert_absolute_path;

#[derive(Deserialize, JsonSchema)]
pub struct FSSearchInput {
    /// The path of the directory to search in (absolute path required). This
    /// directory will be recursively searched.
    pub path: String,
    /// The regular expression pattern to search for. Uses Rust regex syntax.
    pub regex: String,
    /// Glob pattern to filter files (e.g., '*.ts' for TypeScript files). If not
    /// provided, it will search all files (*).
    pub file_pattern: Option<String>,
}

/// Request to perform a regex search on the content across files in a specified
/// directory, providing context-rich results. This tool searches for patterns
/// or specific content across multiple files, displaying each match with
/// encapsulating context. The path must be absolute.
#[derive(ToolDescription)]
pub struct FSSearch;

impl NamedTool for FSSearch {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_fs_search")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for FSSearch {
    type Input = FSSearchInput;

    async fn call(&self, input: Self::Input) -> Result<String, String> {
        let dir = Path::new(&input.path);
        assert_absolute_path(dir)?;

        if !dir.exists() {
            return Err(format!("Directory '{}' does not exist", input.path));
        }

        // Create regex pattern - case-insensitive by default
        let pattern = format!("(?i){}", input.regex);
        let regex = Regex::new(&pattern).map_err(|e| format!("Invalid regex pattern: {}", e))?;

        // TODO: Current implementation is extremely slow and inefficient.
        // It should ideally be taking in a stream of files and processing them
        // concurrently.
        let walker = Walker::max_all().cwd(dir.to_path_buf());

        let files = walker
            .get()
            .await
            .map_err(|e| format!("Failed to walk directory '{}': {}", dir.display(), e))?;

        let mut matches = Vec::new();
        let mut seen_paths = HashSet::new();

        for file in files {
            if file.is_dir() {
                continue;
            }

            let path = Path::new(&file.path);
            let full_path = dir.join(path);

            // Apply file pattern filter if provided
            if let Some(ref pattern) = input.file_pattern {
                let glob = glob::Pattern::new(pattern).map_err(|e| {
                    format!(
                        "Invalid glob pattern '{}' for file '{}': {}",
                        pattern,
                        full_path.display(),
                        e
                    )
                })?;
                if let Some(filename) = path.file_name().unwrap_or(path.as_os_str()).to_str() {
                    if !glob.matches(filename) {
                        continue;
                    }
                }
            }

            // Skip if we've already processed this file
            if !seen_paths.insert(full_path.clone()) {
                continue;
            }

            // Try to read the file content
            let content = match tokio::fs::read_to_string(&full_path).await {
                Ok(content) => content,
                Err(e) => {
                    // Skip binary or unreadable files silently
                    if e.kind() != std::io::ErrorKind::InvalidData {
                        matches.push(format!("Error reading {:?}: {}", full_path.display(), e));
                    }
                    continue;
                }
            };

            // Process the file line by line
            for (line_num, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    // Format match in ripgrep style: filepath:line_num:content
                    matches.push(format!("{}:{}:{}", full_path.display(), line_num + 1, line));
                }
            }
        }

        if matches.is_empty() {
            let output = format!(
                "{} No matches found for pattern '{}' in path '{}'",
                style("Note:").blue().bold(),
                style(&input.regex).yellow(),
                style(&input.path).cyan()
            );
            println!("{}", output);
            Ok(strip_ansi_escapes::strip_str(output))
        } else {
            println!(
                "{}\n{}",
                style("Matches:").dim(),
                RipGrepFormatter(matches.clone()).format(&regex)
            );
            Ok(matches.join("\n"))
        }
    }
}

/// RipGrepFormatter formats search results in ripgrep-like style.
struct RipGrepFormatter(Vec<String>);

impl RipGrepFormatter {
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
    fn format(self, regex: &Regex) -> String {
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
                std::collections::BTreeMap::new(),
                |mut acc: std::collections::BTreeMap<&str, Vec<(&str, &str)>>,
                 (path, num, content)| {
                    acc.entry(path)
                        .or_default()
                        .push((num, content));
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
mod test {
    use pretty_assertions::assert_eq;
    use tokio::fs;

    use super::*;
    use crate::utils::TempDir;

    #[tokio::test]
    async fn test_fs_search_content() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("test1.txt"), "Hello test world")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("test2.txt"), "Another test case")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("other.txt"), "No match here")
            .await
            .unwrap();

        let fs_search = FSSearch;
        let result = fs_search
            .call(FSSearchInput {
                path: temp_dir.path().to_string_lossy().to_string(),
                regex: "test".to_string(),
                file_pattern: None,
            })
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(result.contains("test1.txt"));
        assert!(result.contains("test2.txt"));
    }

    #[tokio::test]
    async fn test_fs_search_with_pattern() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("test1.txt"), "Hello test world")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("test2.rs"), "fn test() {}")
            .await
            .unwrap();

        let fs_search = FSSearch;
        let result = fs_search
            .call(FSSearchInput {
                path: temp_dir.path().to_string_lossy().to_string(),
                regex: "test".to_string(),
                file_pattern: Some("*.rs".to_string()),
            })
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(result.contains("test2.rs"));
    }

    #[tokio::test]
    async fn test_fs_search_with_context() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\ntest line\nline 4\nline 5";

        fs::write(temp_dir.path().join("test.txt"), content)
            .await
            .unwrap();

        let fs_search = FSSearch;
        let result = fs_search
            .call(FSSearchInput {
                path: temp_dir.path().to_string_lossy().to_string(),
                regex: "test".to_string(),
                file_pattern: None,
            })
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(result.contains("test line"));
    }

    #[tokio::test]
    async fn test_fs_search_recursive() {
        let temp_dir = TempDir::new().unwrap();

        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).await.unwrap();

        fs::write(temp_dir.path().join("test1.txt"), "test content")
            .await
            .unwrap();
        fs::write(sub_dir.join("test2.txt"), "more test content")
            .await
            .unwrap();
        fs::write(sub_dir.join("best.txt"), "this is proper\n test content")
            .await
            .unwrap();

        let fs_search = FSSearch;
        let result = fs_search
            .call(FSSearchInput {
                path: temp_dir.path().to_string_lossy().to_string(),
                regex: "test".to_string(),
                file_pattern: None,
            })
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(result.contains("test1.txt"));
        assert!(result.contains("test2.txt"));
        assert!(result.contains("best.txt"));
    }

    #[tokio::test]
    async fn test_fs_search_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("test.txt"),
            "TEST CONTENT\ntest content",
        )
        .await
        .unwrap();

        let fs_search = FSSearch;
        let result = fs_search
            .call(FSSearchInput {
                path: temp_dir.path().to_string_lossy().to_string(),
                regex: "test".to_string(),
                file_pattern: None,
            })
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(result.contains("TEST CONTENT"));
        assert!(result.contains("test content"));
    }

    #[tokio::test]
    async fn test_fs_search_no_matches() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("test.txt"), "content")
            .await
            .unwrap();

        let fs_search = FSSearch;
        let result = fs_search
            .call(FSSearchInput {
                path: temp_dir.path().to_string_lossy().to_string(),
                regex: "nonexistent".to_string(),
                file_pattern: None,
            })
            .await
            .unwrap();

        assert!(result.contains("No matches found"));
    }

    #[tokio::test]
    async fn test_fs_search_invalid_regex() {
        let temp_dir = TempDir::new().unwrap();

        let fs_search = FSSearch;
        let result = fs_search
            .call(FSSearchInput {
                path: temp_dir.path().to_string_lossy().to_string(),
                regex: "[invalid".to_string(),
                file_pattern: None,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid regex pattern"));
    }

    #[tokio::test]
    async fn test_fs_search_relative_path() {
        let fs_search = FSSearch;
        let result = fs_search
            .call(FSSearchInput {
                path: "relative/path".to_string(),
                regex: "test".to_string(),
                file_pattern: None,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path must be absolute"));
    }

    #[cfg(test)]
    mod rip_grep_formatter_tests {
        use pretty_assertions::assert_eq;
        use regex::Regex;

        use crate::fs::fs_find::RipGrepFormatter;

        #[test]
        fn test_ripgrep_formatter_single_file() {
            let input = vec!["file.txt:1:first match", "file.txt:2:second match"]
                .into_iter()
                .map(String::from)
                .collect();

            let formatter = RipGrepFormatter(input);
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

            let formatter = RipGrepFormatter(input);
            let result = formatter.format(&Regex::new("file").unwrap());
            let actual = strip_ansi_escapes::strip_str(&result);

            let expected = "file1.txt\n1:match in file1\n\nfile2.txt\n1:first match in file2\n2:second match in file2\n\nfile3.txt\n1:match in file3\n";
            assert_eq!(actual, expected);
        }

        #[test]
        fn test_ripgrep_formatter_empty_input() {
            let formatter = RipGrepFormatter(vec![]);
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

            let formatter = RipGrepFormatter(input);
            let result = formatter.format(&Regex::new("match").unwrap());
            let actual = strip_ansi_escapes::strip_str(&result);

            let expected = "file.txt\n1:valid match\n2:another valid match\n";
            assert_eq!(actual, expected);
        }
    }
}
