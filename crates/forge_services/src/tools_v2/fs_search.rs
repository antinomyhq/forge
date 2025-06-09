use crate::utils::{assert_absolute_path, format_display_path};
use crate::Infrastructure;
use anyhow::Context;
use forge_app::{EnvironmentService, FsSearchService, SearchResult};
use forge_display::{GrepFormat, TitleFormat};
use forge_domain::ToolDescription;
use forge_tool_macros::ToolDescription;
use forge_walker::Walker;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

const MAX_SEARCH_LINE_LIMIT: u64 = 200;

// Using FSSearchInput from forge_domain

// Helper to handle FSSearchInput functionality
struct FSSearchHelper<'a> {
    path: &'a str,
    regex: Option<&'a String>,
    file_pattern: Option<&'a String>,
}

impl FSSearchHelper<'_> {
    fn path(&self) -> &str {
        &self.path
    }

    fn regex(&self) -> Option<&String> {
        self.regex
    }

    fn file_pattern(&self) -> Option<&String> {
        self.file_pattern
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
#[derive(ToolDescription)]
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

    fn create_title(
        &self,
        path: &str,
        regex: Option<&String>,
        file_pattern: Option<&String>,
    ) -> anyhow::Result<TitleFormat> {
        // Format the title with relative path if possible
        let formatted_dir = self.format_display_path(path.as_ref())?;
        let helper = FSSearchHelper { path, regex, file_pattern };

        let title = match (&helper.regex(), &helper.file_pattern()) {
            (Some(regex), Some(pattern)) => {
                format!("Search for '{regex}' in '{pattern}' files at {formatted_dir}")
            }
            (Some(regex), None) => format!("Search for '{regex}' at {formatted_dir}"),
            (None, Some(pattern)) => format!("Search for '{pattern}' at {formatted_dir}"),
            (None, None) => format!("Search at {formatted_dir}"),
        };

        Ok(TitleFormat::debug(title))
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsSearchService for ForgeFsSearch<F> {
    async fn search(
        &self,
        input_path: String,
        input_regex: Option<String>,
        file_pattern: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let helper = FSSearchHelper {
            path: &input_path,
            regex: input_regex.as_ref(),
            file_pattern: file_pattern.as_ref(),
        };

        let file_path = Path::new(helper.path());
        assert_absolute_path(file_path)?;

        let _title_format = self.create_title(&input_path, input_regex.as_ref(), file_pattern.as_ref())?;
        
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

        let paths = retrieve_file_paths(file_path).await?;

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
            // return Ok("No matches found.".to_string());
            todo!()
        }

        let mut _formatted_output = GrepFormat::new(matches.clone());

        // Use GrepFormat for content search, simple list for filename search
        if let Some(regex) = regex {
            _formatted_output = _formatted_output.regex(regex);
        }
        let total_lines = matches.join("\n").lines().count() as u64;
        if total_lines > MAX_SEARCH_LINE_LIMIT {
            let _limited_matches = matches
                .iter()
                .take(MAX_SEARCH_LINE_LIMIT as usize)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            // let path = self
            //     .0
            //     .file_write_service()
            //     .write_temp("forge_find_", ".md", &matches)
            //     .await?;
        }

        todo!()
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
