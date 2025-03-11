use std::path::{Path, PathBuf};

use forge_walker::Walker;
use nu_ansi_term::Color;
use reedline::{Completer, Suggestion};

use crate::completer::search_term::SearchTerm;
use crate::completer::CommandCompleter;

#[derive(Clone)]
pub struct InputCompleter {
    cwd: PathBuf,
}

impl InputCompleter {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    /// Get path suggestions for the given query
    ///
    /// For directories, appends a trailing slash
    /// For files, doesn't add any trailing character
    fn get_path_suggestions(&self, query: &str, span: reedline::Span) -> Vec<Suggestion> {
        // Special case: query ends with a directory separator, we want to show contents
        // of this directory
        if query.ends_with('/') || query.ends_with('\\') {
            return self.get_directory_contents(query, span);
        }

        // Determine if we're dealing with an absolute or relative path
        let (base_path, search_term) = if let Some(query_path) = Path::new(query).parent() {
            if query_path.as_os_str().is_empty() {
                // Just a file/dir name without path separators
                (
                    self.cwd.clone(),
                    Path::new(query)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                )
            } else if Path::new(query).is_absolute() {
                // Absolute path
                (
                    query_path.to_path_buf(),
                    Path::new(query)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                )
            } else {
                // Relative path with directory components
                (
                    self.cwd.join(query_path),
                    Path::new(query)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                )
            }
        } else {
            // Empty string or just root
            (self.cwd.clone(), query.to_string())
        };

        // Use the walker to get all files and directories
        let walker = Walker::min_all().max_depth(1).cwd(base_path);
        let files = walker.get_blocking().unwrap_or_default();

        // Filter based on the search term and create suggestions
        files
            .into_iter()
            .filter(|file| !file.is_cwd) // skip current dir
            .filter_map(|file| {
                let file_name = file.file_name.as_ref()?;
                let file_name_lower = file_name.to_lowercase();
                let search_lower = search_term.to_lowercase();

                // If search term is empty, show all files and dirs
                // Otherwise, filter based on the search term
                if search_lower.is_empty() || file_name_lower.contains(&search_lower) {
                    // Determine the value to return based on the original query and matched file
                    let mut value = if let Some(parent) = Path::new(query).parent() {
                        if parent.as_os_str().is_empty() {
                            file_name.to_string()
                        } else {
                            format!("{}/{}", parent.display(), file_name)
                        }
                    } else {
                        file_name.to_string()
                    };

                    // Add trailing slash for directories
                    if file.is_dir() {
                        value.push('/');

                        // Only append whitespace for files, not for directories
                        return Some(Suggestion {
                            value,
                            description: None,
                            style: Some(Color::Rgb(1, 158, 159).normal()), // #019E9F in RGB
                            extra: None,
                            span,
                            append_whitespace: false,
                        });
                    }

                    Some(Suggestion {
                        value,
                        description: None,
                        style: Some(Color::Rgb(178, 180, 187).normal()), // #B2B4BB in RGB
                        extra: None,
                        span,
                        append_whitespace: true,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get contents of a directory for queries ending with '/'
    fn get_directory_contents(&self, query: &str, span: reedline::Span) -> Vec<Suggestion> {
        // Determine the target directory
        let target_dir = if Path::new(query).is_absolute() {
            Path::new(query).to_path_buf()
        } else {
            self.cwd.join(query)
        };

        // Normalize path (remove trailing slash for the walker)
        let target_dir = if query.ends_with('/') || query.ends_with('\\') {
            let without_trailing_slash = query.trim_end_matches(['/', '\\']);
            if without_trailing_slash.is_empty() {
                // Root directory case
                PathBuf::from("/")
            } else if Path::new(without_trailing_slash).is_absolute() {
                Path::new(without_trailing_slash).to_path_buf()
            } else {
                self.cwd.join(without_trailing_slash)
            }
        } else {
            target_dir
        };

        // Use the walker to get all files and directories
        let walker = Walker::min_all().max_depth(1).cwd(target_dir);
        let files = walker.get_blocking().unwrap_or_default();

        // Create suggestions for all files in the directory
        files
            .into_iter()
            .filter(|file| !file.is_cwd) // skip current dir
            .filter_map(|file| {
                if let Some(file_name) = file.file_name.as_ref() {
                    let mut value = format!("{}{}", query, file_name);

                    // Add trailing slash for directories
                    if file.is_dir() {
                        value.push('/');

                        return Some(Suggestion {
                            value,
                            description: None,
                            style: Some(Color::Rgb(1, 158, 159).normal()), // #019E9F in RGB
                            extra: None,
                            span,
                            append_whitespace: false,
                        });
                    }

                    Some(Suggestion {
                        value,
                        description: None,
                        style: Some(Color::Rgb(178, 180, 187).normal()), // #B2B4BB in RGB
                        extra: None,
                        span,
                        append_whitespace: true,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Completer for InputCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        // Handle command completion (starts with '/')
        if line.starts_with("/") {
            let result = CommandCompleter.complete(line, pos);
            if !result.is_empty() {
                return result;
            }
        }

        // Get the search term from the line
        if let Some(query) = SearchTerm::new(line, pos).process() {
            // Get matching files and directories
            self.get_path_suggestions(query.term, query.span)
        } else {
            vec![]
        }
    }
}
