use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_display::{GrepFormat, TitleFormat};
use forge_domain::{
    EnvironmentService, ExecutableTool, FSSearchInput, NamedTool, ToolCallContext, ToolDescription,
    ToolName, ToolOutput,
};
use forge_tool_macros::ToolDescription;
use forge_walker::Walker;
use regex::Regex;

use crate::metadata::Metadata;
use crate::utils::{assert_absolute_path, format_display_path};
use crate::{Clipper, FsWriteService, Infrastructure};

const MAX_SEARCH_CHAR_LIMIT: usize = 40_000;

// Using FSSearchInput from forge_domain

// Helper to handle FSSearchInput functionality
struct FSSearchHelper<'a>(&'a FSSearchInput);

impl FSSearchHelper<'_> {
    fn path(&self) -> &str {
        &self.0.path
    }

    fn regex(&self) -> Option<&String> {
        self.0.regex.as_ref()
    }

    fn file_pattern(&self) -> Option<&String> {
        self.0.file_pattern.as_ref()
    }

    fn get_file_pattern(&self) -> anyhow::Result<Option<glob::Pattern>> {
        Ok(match &self.0.file_pattern {
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
/// patterns across projects. For large pages, returns the first 40,000
/// characters and stores the complete content in a temporary file for
/// subsequent access.
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

    fn create_title(&self, input: &FSSearchInput) -> anyhow::Result<TitleFormat> {
        // Format the title with relative path if possible
        let formatted_dir = self.format_display_path(input.path.as_ref())?;
        let helper = FSSearchHelper(input);

        let title = match (&helper.regex(), &helper.file_pattern()) {
            (Some(regex), Some(pattern)) => {
                format!("Search for '{regex}' in '{pattern}' files at {formatted_dir}")
            }
            (Some(regex), None) => format!("Search for '{regex}' at {formatted_dir}"),
            (None, Some(pattern)) => format!("Search for '{pattern}' at {formatted_dir}"),
            (None, None) => format!("at {formatted_dir}"),
        };

        Ok(TitleFormat::debug(title))
    }

    async fn call_inner(
        &self,
        context: ToolCallContext,
        input: FSSearchInput,
        max_char_limit: usize,
    ) -> anyhow::Result<String> {
        let helper = FSSearchHelper(&input);
        let path = Path::new(helper.path());
        assert_absolute_path(path)?;

        let title_format = self.create_title(&input)?;

        context.send_text(title_format).await?;

        // Create content regex pattern if provided
        let regex = match helper.regex() {
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
            if !helper.match_file_path(path.as_path())? {
                continue;
            }

            // File name only search mode
            if regex.is_none() {
                matches.push((self.format_display_path(&path)?).to_string());
                continue;
            }

            // Content matching mode - read and search file contents
            let content = match forge_fs::ForgeFS::read_to_string(&path).await {
                Ok(content) => content,
                Err(e) => {
                    // Skip binary or unreadable files silently
                    if let Some(e) = e
                        .downcast_ref::<std::io::ErrorKind>()
                        .map(|e| std::io::ErrorKind::InvalidData.eq(e))
                    {
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
                if !found_match && helper.regex().is_some() {
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

        let matches = matches.join("\n");
        let metadata = Metadata::default()
            .add("path", input.path)
            .add_optional("regex", input.regex)
            .add_optional("file_pattern", input.file_pattern)
            .add("total_chars", matches.len())
            .add("start_char", 0);

        let truncated_result = Clipper::from_start(max_char_limit).clip(&matches);
        if let Some(truncated) = truncated_result.prefix_content() {
            let path = self
                .0
                .file_write_service()
                .write_temp("forge_find_", ".md", &matches)
                .await?;

            let metadata = metadata
                .add("end_char", truncated.len())
                .add("temp_file", path.display());

            let truncation_tag = format!("\n<truncation>content is truncated to {} chars, remaining content can be read from path:{}</truncation>", 
            max_char_limit,path.to_string_lossy());

            Ok(format!("{metadata}{truncated}{truncation_tag}"))
        } else {
            let metadata = metadata.add("end_char", matches.len());
            Ok(format!("{metadata}{matches}"))
        }
    }
}

async fn retrieve_file_paths(dir: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    if dir.is_dir() {
        // note: Paths needs mutable to avoid flaky tests.
        #[allow(unused_mut)]
        let mut paths = Walker::max_all()
            .cwd(dir.to_path_buf())
            .get()
            .await
            .with_context(|| format!("Failed to walk directory '{}'", dir.display()))?
            .into_iter()
            .map(|file| dir.join(file.path))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        #[cfg(test)]
        paths.sort();

        Ok(paths)
    } else {
        Ok(Vec::from_iter([dir.to_path_buf()]))
    }
}

impl<F> NamedTool for FSFind<F> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_fs_search")
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for FSFind<F> {
    type Input = FSSearchInput;

    async fn call(
        &self,
        context: ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolOutput> {
        let result = self
            .call_inner(context, input, MAX_SEARCH_CHAR_LIMIT)
            .await?;
        Ok(ToolOutput::text(result))
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use tokio::fs;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::utils::{TempDir, ToolContentExtension};

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
                FSSearchInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

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
                FSSearchInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: Some("*.rs".to_string()),
                },
            )
            .await
            .unwrap();

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
                FSSearchInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: None,
                    file_pattern: Some("test*.txt".to_string()),
                },
            )
            .await
            .unwrap();

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
                FSSearchInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

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
                FSSearchInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

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
                FSSearchInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("test".to_string()),
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

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
                FSSearchInput {
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
                FSSearchInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: None,
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

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
                FSSearchInput {
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
                FSSearchInput {
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
                FSSearchInput {
                    path: temp_dir.path().join("best.txt").display().to_string(),
                    regex: Some("nice".to_string()),
                    file_pattern: None,
                },
            )
            .await
            .unwrap();

        assert!(result.contains(&format!(
            "{}:1:nice code.",
            temp_dir.path().join("best.txt").display()
        )));

        // case 2: check if file is present or not by using search tool.
        let result = fs_search
            .call(
                ToolCallContext::default(),
                FSSearchInput {
                    path: temp_dir.path().join("best.txt").display().to_string(),
                    regex: None,
                    file_pattern: None,
                },
            )
            .await
            .unwrap();
        assert!(result.contains(&format!("{}", temp_dir.path().join("best.txt").display())));
    }

    #[tokio::test]
    async fn test_fs_large_result() {
        let temp_dir = TempDir::new().unwrap();

        let content = "content".repeat(10);
        fs::write(temp_dir.path().join("file1.txt"), &content)
            .await
            .unwrap();

        let infra = Arc::new(MockInfrastructure::new());
        let fs_search = FSFind::new(infra);
        let result = fs_search
            .call_inner(
                ToolCallContext::default(),
                FSSearchInput {
                    path: temp_dir.path().to_string_lossy().to_string(),
                    regex: Some("content*".into()),
                    file_pattern: None,
                },
                100,
            )
            .await
            .unwrap();
        assert!(result.contains("content is truncated to 100 chars"))
    }
}
