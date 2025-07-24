use std::path::PathBuf;
use std::sync::Arc;

use forge_walker::Walker;
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32Str};
use reedline::{Completer, Suggestion};

use crate::completer::CommandCompleter;
use crate::completer::search_term::SearchTerm;
use crate::model::ForgeCommandManager;

pub struct InputCompleter {
    walker: Walker,
    command: CommandCompleter,
    fuzzy_matcher: Matcher,
}

impl InputCompleter {
    pub fn new(cwd: PathBuf, command_manager: Arc<ForgeCommandManager>) -> Self {
        let walker = Walker::max_all().cwd(cwd).skip_binary(true);
        Self {
            walker,
            command: CommandCompleter::new(command_manager),
            fuzzy_matcher: Matcher::new(Config::DEFAULT),
        }
    }
}

impl Completer for InputCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        if line.starts_with("/") {
            // if the line starts with '/' it's probably a command, so we delegate to the
            // command completer.
            let result = self.command.complete(line, pos);
            if !result.is_empty() {
                return result;
            }
        }

        if let Some(query) = SearchTerm::new(line, pos).process() {
            let files = self.walker.get_blocking().unwrap_or_default();
            let mut scored_matches: Vec<(u32, Suggestion)> = files
                .into_iter()
                .filter(|file| !file.is_dir())
                .filter_map(|file| {
                    if let Some(file_name) = file.file_name.as_ref() {
                        let mut haystack_buf = Vec::new();
                        let haystack = Utf32Str::new(file_name, &mut haystack_buf);
                        let pattern =
                            Pattern::parse(query.term, CaseMatching::Ignore, Normalization::Smart);

                        if let Some(score) = pattern.score(haystack, &mut self.fuzzy_matcher) {
                            let path_md_fmt = format!("[{}]", file.path);
                            Some((
                                score,
                                Suggestion {
                                    description: None,
                                    value: path_md_fmt,
                                    style: None,
                                    extra: None,
                                    span: query.span,
                                    append_whitespace: true,
                                },
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            // Sort by fuzzy match score (higher is better)
            scored_matches.sort_by(|a, b| b.0.cmp(&a.0));

            // Extract suggestions from scored matches
            scored_matches
                .into_iter()
                .map(|(_, suggestion)| suggestion)
                .collect()
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::TempDir;

    use super::*;
    use crate::model::ForgeCommandManager;

    fn create_test_fixture() -> (TempDir, InputCompleter) {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create test files
        fs::write(temp_path.join("config.rs"), "").unwrap();
        fs::write(temp_path.join("main.rs"), "").unwrap();
        fs::write(temp_path.join("lib.rs"), "").unwrap();
        fs::write(temp_path.join("test_file.txt"), "").unwrap();
        fs::write(temp_path.join("another_config.toml"), "").unwrap();

        let command_manager = Arc::new(ForgeCommandManager::default());
        let completer = InputCompleter::new(temp_path, command_manager);

        (temp_dir, completer)
    }

    #[test]
    fn test_fuzzy_matching_works() {
        let (_temp_dir, mut completer) = create_test_fixture();

        // Test fuzzy matching - "cfg" should match "config.rs"
        let actual = completer.complete("@cfg", 4);

        // Should find config.rs and another_config.toml
        assert!(actual.len() >= 1);
        let config_match = actual.iter().find(|s| s.value.contains("config.rs"));
        assert!(
            config_match.is_some(),
            "Should find config.rs with fuzzy matching"
        );
    }

    #[test]
    fn test_fuzzy_matching_ordering() {
        let (_temp_dir, mut completer) = create_test_fixture();

        // Test that better matches come first
        let actual = completer.complete("@config", 7);

        // config.rs should rank higher than another_config.toml for "config" query
        assert!(actual.len() >= 2);
        let first_match = &actual[0];
        assert!(
            first_match.value.contains("config.rs"),
            "config.rs should be the top match for 'config' query, got: {}",
            first_match.value
        );
    }

    #[test]
    fn test_literal_fallback() {
        let (_temp_dir, mut completer) = create_test_fixture();

        // Test that literal matching still works for exact substrings
        let actual = completer.complete("@main", 5);

        assert!(actual.len() >= 1);
        let main_match = actual.iter().find(|s| s.value.contains("main.rs"));
        assert!(
            main_match.is_some(),
            "Should find main.rs with literal matching"
        );
    }
}
