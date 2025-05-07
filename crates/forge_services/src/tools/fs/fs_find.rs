use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_display::{GrepFormat, TitleFormat};
use forge_domain::{
    EnvironmentService, ExecutableTool, NamedTool, ToolCallContext, ToolDescription, ToolName,
};
use forge_tool_macros::ToolDescription;
use forge_walker::Walker;
use regex::Regex;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::utils::{assert_absolute_path, format_display_path};
use crate::Infrastructure;

#[derive(Deserialize, JsonSchema)]
pub struct FSFindInput {
    /// The absolute path of the directory or file to search in. If it's a
    /// directory, it will be searched recursively. If it's a file path,
    /// only that specific file will be searched.
    pub path: String,

    /// The regular expression pattern to search for in file contents.
    /// Uses Rust regex syntax. If not provided, only file name matching will be
    /// performed.
    pub regex: Option<String>,

    /// Glob pattern to filter files (e.g., '*.ts' for TypeScript files). If not
    /// provided, it will search all files (*).
    pub file_pattern: Option<String>,
}

impl FSFindInput {
    fn get_file_pattern(&self) -> anyhow::Result<Option<glob::Pattern>> {
        Ok(match &self.file_pattern {
            Some(pattern) => Some(
                glob::Pattern::new(pattern)
                    .with_context(|| format!("Invalid glob pattern: {pattern}"))?,
            ),
            None => None,
        })
    }

    fn match_file_path(&self, path: &Path) -> anyhow::Result<bool> {
        // Don't process directories
        if path.is_dir() {
            return Ok(false);
        }

        // If no pattern is specified, match all files
        let pattern = self.get_file_pattern()?;
        if pattern.is_none() {
            return Ok(true);
        }

        // Otherwise, check if the file matches the pattern
        Ok(path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| !name.is_empty() && pattern.unwrap().matches(name)))
    }
}

/// Recursively searches directories for files by content (regex) and/or name
/// (glob pattern). Provides context-rich results with line numbers for content
/// matches. Two modes: content search (when regex provided) or file finder
/// (when regex omitted). Uses case-insensitive Rust regex syntax. Requires
/// absolute paths. Avoids binary files and excluded directories. Best for code
/// exploration, API usage discovery, configuration settings, or finding
/// patterns across projects.
#[derive(ToolDescription)]
pub struct FSFind<F>(Arc<F>);

impl<F: Infrastructure> FSFind<F> {
    pub fn new(f: Arc<F>) -> Self {
        Self(f)
    }

    /// Formats a path for display, converting absolute paths to relative when
    /// possible
    ///
    /// If the path starts with the current working directory, returns a
    /// relative path. Otherwise, returns the original absolute path.
    fn format_display_path(&self, path: &Path) -> anyhow::Result<String> {
        // Get the current working directory
        let env = self.0.environment_service().get_environment();
        let cwd = env.cwd.as_path();

        // Use the shared utility function
        format_display_path(path, cwd)
    }

    fn create_title(&self, input: &FSFindInput) -> anyhow::Result<TitleFormat> {
        // Format the title with relative path if possible
        let formatted_dir = self.format_display_path(input.path.as_ref())?;

        let title = match (&input.regex, &input.file_pattern) {
            (Some(regex), Some(pattern)) => {
                format!("Search for '{regex}' in '{pattern}' files at {formatted_dir}")
            }
            (Some(regex), None) => format!("Search for '{regex}' at {formatted_dir}"),
            (None, Some(pattern)) => format!("Search for '{pattern}' at {formatted_dir}"),
            (None, None) => format!("at {formatted_dir}"),
        };

        Ok(TitleFormat::debug(title))
    }

    async fn call(&self, context: ToolCallContext, input: FSFindInput) -> anyhow::Result<String> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        let title_format = self.create_title(&input)?;

        context.send_text(title_format).await?;

        // Create content regex pattern if provided
        let regex = match &input.regex {
            Some(regex) => {
                let pattern = format!("(?i){regex}"); // Case-insensitive by default
                Some(
                    Regex::new(&pattern)
                        .with_context(|| format!("Invalid regex pattern: {regex}"))?,
                )
            }
            None => None,
        };

        let paths = retrieve_file_paths(path).await?;

        let mut matches = Vec::new();

        for path in paths {
            if !input.match_file_path(path.as_path())? {
                continue;
            }

            // File name only search mode
            if regex.is_none() {
                matches.push((self.format_display_path(&path)?).to_string());
                continue;
            }

            // Content matching mode - read and search file contents
            let content = match tokio::fs::read_to_string(&path).await {
                Ok(content) => content,
                Err(e) => {
                    // Skip binary or unreadable files silently
                    if e.kind() != std::io::ErrorKind::InvalidData {
                        matches.push(format!(
                            "Error reading {}: {}",
                            self.format_display_path(&path)?,
                            e
                        ));
                    }
                    continue;
                }
            };

            // Process the file line by line to find content matches
            if let Some(regex) = &regex {
                let mut found_match = false;

                for (line_num, line) in content.lines().enumerate() {
                    if regex.is_match(line) {
                        found_match = true;
                        // Format match in ripgrep style: filepath:line_num:content
                        matches.push(format!(
                            "{}:{}:{}",
                            self.format_display_path(&path)?,
                            line_num + 1,
                            line
                        ));
                    }
                }

                // If no matches found in content but we're looking for content,
                // don't add this file to matches
                if !found_match && input.regex.is_some() {
                    continue;
                }
            }
        }

        // Format and return results
        if matches.is_empty() {
            return Ok("No matches found.".to_string());
        }

        let mut formatted_output = GrepFormat::new(matches.clone());

        // Use GrepFormat for content search, simple list for filename search
        if let Some(regex) = regex {
            formatted_output = formatted_output.regex(regex);
        }

        context.send_text(formatted_output.format()).await?;
        Ok(matches.join("\n"))
    }
}

async fn retrieve_file_paths(dir: &Path) -> anyhow::Result<HashSet<std::path::PathBuf>> {
    if dir.is_dir() {
        Ok(Walker::max_all()
            .cwd(dir.to_path_buf())
            .get()
            .await
            .with_context(|| format!("Failed to walk directory '{}'", dir.display()))?
            .into_iter()
            .map(|file| dir.join(file.path))
            .collect::<HashSet<_>>())
    } else {
        Ok(HashSet::from_iter([dir.to_path_buf()]))
    }
}

impl<F> NamedTool for FSFind<F> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_fs_search")
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for FSFind<F> {
    type Input = FSFindInput;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        self.call(context, input).await
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use tokio::fs;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::tools::utils::TempDir;

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

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
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

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: Some("*.rs".to_string()),
                },
            )
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(result.contains("test2.rs"));
    }

    #[tokio::test]
    async fn test_fs_search_filenames_only() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("test1.txt"), "Hello world")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("test2.txt"), "Another case")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("other.txt"), "No match here")
            .await
            .unwrap();

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: None,
                    file_pattern: Some("test*.txt".to_string()),
                },
            )
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(result.contains("test1.txt"));
        assert!(result.contains("test2.txt"));
        assert!(!result.contains("other.txt"));
    }

    #[tokio::test]
    async fn test_fs_search_with_context() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\ntest line\nline 4\nline 5";

        fs::write(temp_dir.path().join("test.txt"), content)
            .await
            .unwrap();

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
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

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
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

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
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

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("nonexistent".to_string()),
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

        assert!(result.contains("No matches found."));
    }

    #[tokio::test]
    async fn test_fs_search_list_all_files() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("file1.txt"), "content1")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("file2.rs"), "content2")
            .await
            .unwrap();

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: None,
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(result.contains("file1.txt"));
        assert!(result.contains("file2.rs"));
    }

    #[tokio::test]
    async fn test_fs_search_invalid_regex() {
        let temp_dir = TempDir::new().unwrap();

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("[invalid".to_string()),
                    file_pattern: None,
                },
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid regex pattern"));
    }

    #[tokio::test]
    async fn test_fs_search_relative_path() {
        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: "relative/path".to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }
    #[tokio::test]
    async fn test_format_display_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create a mock infrastructure with controlled cwd
        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);

        // Test with a mock path
        let display_path = fs_search.format_display_path(Path::new(&file_path));

        // Since MockInfrastructure has a fixed cwd of "/test",
        // and our temp path won't start with that, we expect the full path
        assert!(display_path.is_ok());
        assert_eq!(display_path.unwrap(), file_path.display().to_string());
    }

    #[tokio::test]
    async fn test_fs_search_in_specific_file() {
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

        fs::write(temp_dir.path().join("best.txt"), "nice code.")
            .await
            .unwrap();

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);

        // case 1: search within a specific file
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().join("best.txt").display().to_string(),
                    regex: Some("nice".to_string()),
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].eq(&format!(
            "{}:1:nice code.",
            temp_dir.path().join("best.txt").display()
        )));

        // case 2: check if file is present or not by using search tool.
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSFindInput {
                    path: temp_dir.path().join("best.txt").display().to_string(),
                    regex: None,
                    file_pattern: None,
                },
            )
            .await
            .unwrap();
        let lines: Vec<_> = result.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].eq(&format!("{}", temp_dir.path().join("best.txt").display())));
    }
}
