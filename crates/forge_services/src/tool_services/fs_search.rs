use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use anyhow::Context;
use forge_app::{FsSearchService, Match, MatchResult, SearchResult};
use forge_walker::Walker;
use grep_searcher::sinks::UTF8;

use crate::utils::assert_absolute_path;

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
#[derive(Default)]
pub struct ForgeFsSearch;

impl ForgeFsSearch {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl FsSearchService for ForgeFsSearch {
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
                    grep_regex::RegexMatcher::new(&pattern)
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
                matches.push(Match { path: path.to_string_lossy().to_string(), result: None });
                continue;
            }

            // Process the file line by line to find content matches
            if let Some(regex) = &regex {
                let mut searcher = grep_searcher::Searcher::new();
                let path_string = path.to_string_lossy().to_string();

                let file = File::open(path)?;
                let mut found_match = false;
                searcher.search_file(
                    regex,
                    &file,
                    UTF8(|line_num, line| {
                        found_match = true;
                        matches.push(Match {
                            path: path_string.clone(),
                            result: Some(MatchResult::Found {
                                line_number: line_num as usize,    /* grep_searcher already
                                                                    * returns
                                                                    * 1-based line numbers */
                                line: line.trim_end().to_string(), // Remove trailing newline
                            }),
                        });

                        Ok(true)
                    }),
                )?;

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
mod test {
    use tokio::fs;

    use super::*;
    use crate::utils::TempDir;

    async fn create_simple_test_directory() -> anyhow::Result<TempDir> {
        let temp_dir = TempDir::new()?;

        fs::write(temp_dir.path().join("test.txt"), "hello test world").await?;
        fs::write(temp_dir.path().join("other.txt"), "no match here").await?;
        fs::write(temp_dir.path().join("code.rs"), "fn test() {}").await?;

        Ok(temp_dir)
    }

    #[tokio::test]
    async fn test_search_content_with_regex() {
        let fixture = create_simple_test_directory().await.unwrap();
        let actual = ForgeFsSearch::new()
            .search(
                fixture.path().to_string_lossy().to_string(),
                Some("test".to_string()),
                None,
            )
            .await
            .unwrap();

        assert!(actual.is_some());
    }

    #[tokio::test]
    async fn test_search_file_pattern_only() {
        let fixture = create_simple_test_directory().await.unwrap();
        let actual = ForgeFsSearch::new()
            .search(
                fixture.path().to_string_lossy().to_string(),
                None,
                Some("*.rs".to_string()),
            )
            .await
            .unwrap();

        assert!(actual.is_some());
        let result = actual.unwrap();
        assert!(result.matches.iter().all(|m| m.path.ends_with(".rs")));
        assert!(result.matches.iter().all(|m| m.result.is_none())); // File pattern only = no content result
    }

    #[tokio::test]
    async fn test_search_combined_pattern_and_content() {
        let fixture = create_simple_test_directory().await.unwrap();
        let actual = ForgeFsSearch::new()
            .search(
                fixture.path().to_string_lossy().to_string(),
                Some("test".to_string()),
                Some("*.rs".to_string()),
            )
            .await
            .unwrap();

        assert!(actual.is_some());
        let result = actual.unwrap();
        assert!(result.matches.iter().all(|m| m.path.ends_with(".rs")));
        assert!(result.matches.iter().all(|m| m.result.is_some())); // Content search = has content result
    }

    #[tokio::test]
    async fn test_search_single_file() {
        let fixture = create_simple_test_directory().await.unwrap();
        let file_path = fixture.path().join("test.txt");
        let actual = ForgeFsSearch::new()
            .search(
                file_path.to_string_lossy().to_string(),
                Some("hello".to_string()),
                None,
            )
            .await
            .unwrap();

        assert!(actual.is_some());
    }

    #[tokio::test]
    async fn test_search_no_matches() {
        let fixture = create_simple_test_directory().await.unwrap();
        let actual = ForgeFsSearch::new()
            .search(
                fixture.path().to_string_lossy().to_string(),
                Some("nonexistent".to_string()),
                None,
            )
            .await
            .unwrap();

        assert!(actual.is_none());
    }

    #[tokio::test]
    async fn test_search_pattern_no_matches() {
        let fixture = create_simple_test_directory().await.unwrap();
        let actual = ForgeFsSearch::new()
            .search(
                fixture.path().to_string_lossy().to_string(),
                None,
                Some("*.cpp".to_string()),
            )
            .await
            .unwrap();

        assert!(actual.is_none());
    }

    #[tokio::test]
    async fn test_search_nonexistent_path() {
        let result = ForgeFsSearch::new()
            .search(
                "/nonexistent/path".to_string(),
                Some("test".to_string()),
                None,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_search_relative_path_error() {
        let result = ForgeFsSearch::new()
            .search("relative/path".to_string(), Some("test".to_string()), None)
            .await;

        assert!(result.is_err());
    }
}
