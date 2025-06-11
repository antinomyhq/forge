use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_app::{EnvironmentService, FsSearchService, SearchResult};
use forge_domain::Environment;
use forge_walker::Walker;
use regex::Regex;

use crate::utils::{assert_absolute_path, format_display_path};
use crate::Infrastructure;

// Using FSSearchInput from forge_domain

// Helper to handle FSSearchInput functionality
struct FSSearchHelper<'a> {
    path: &'a str,
    regex: Option<&'a String>,
    file_pattern: Option<&'a String>,
}

impl FSSearchHelper<'_> {
    fn path(&self) -> &str {
        self.path
    }

    fn regex(&self) -> Option<&String> {
        self.regex
    }

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
/// patterns across projects. For large pages, returns the first 200
/// lines and stores the complete content in a temporary file for
/// subsequent access.
pub struct ForgeFsSearch<F>(Arc<F>);

impl<F: Infrastructure> ForgeFsSearch<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
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
    async fn search(
        &self,
        input_path: String,
        input_regex: Option<String>,
        file_pattern: Option<String>,
    ) -> anyhow::Result<Option<SearchResult>> {
        let helper = FSSearchHelper {
            path: &input_path,
            regex: input_regex.as_ref(),
            file_pattern: file_pattern.as_ref(),
        };

        let path = Path::new(helper.path());
        assert_absolute_path(path)?;

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
        if matches.is_empty() {
            return Ok(None);
        }

        Ok(Some(SearchResult { matches }))
    }
}

impl<F> ForgeFsSearch<F> {
    fn truncate(
        result: anyhow::Result<Option<SearchResult>>,
        start_result: Option<u64>,
        env: Environment,
    ) -> anyhow::Result<Option<SearchResult>> {
        let start_line = start_result.unwrap_or(0);
        let max_lines = env.max_search_lines;

        let result = result?;
        if let Some(result) = result {
            let truncated_matches: Vec<String> = result
                .matches
                .into_iter()
                .skip(start_line as usize)
                .take(max_lines as usize)
                .collect();

            Ok(Some(SearchResult { matches: truncated_matches }))
        } else {
            Ok(None)
        }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsSearchService for ForgeFsSearch<F> {
    async fn search(
        &self,
        input_path: String,
        input_regex: Option<String>,
        file_pattern: Option<String>,
        start_result: Option<u64>,
    ) -> anyhow::Result<Option<SearchResult>> {
        let env = self.0.environment_service().get_environment();

        Self::truncate(
            self.search(input_path, input_regex, file_pattern).await,
            start_result,
            env,
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachment::tests::MockInfrastructure;

    #[tokio::test]
    async fn test_fs_search_with_truncation() {
        // Create more than SEARCH_MAX_LINES (25 for tests) to trigger truncation
        let mut matches = Vec::new();
        for i in 1..=26 {
            matches.push(format!(
                "file{i}.txt:{i}:This is line {i} with search pattern"
            ));
        }

        // Add content that should be truncated and not appear in response
        let truncated_content = "file_truncated.txt:999:This should be truncated and not appear";
        matches.push(truncated_content.to_string());
        let infra = MockInfrastructure::new();

        let actual = ForgeFsSearch::<()>::truncate(
            Ok(Some(SearchResult { matches })),
            Some(1),
            infra.environment_service().get_environment(),
        )
        .ok()
        .flatten()
        .unwrap();

        // verify that there is no truncated content in the results
        assert!(!actual.matches.iter().any(|m| m == truncated_content));

        insta::assert_debug_snapshot!(actual);
    }
}
