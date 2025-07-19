use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_walker::Walker;
use reedline::{Completer, Suggestion};

use crate::completer::CommandCompleter;
use crate::completer::search_term::SearchTerm;
use crate::model::ForgeCommandManager;

/// Maximum number of completions to show
/// This is a limit to prevent overwhelming the user with too many suggestions.
const SHOW_MAX_COMPLETIONS: usize = 200;

#[derive(Clone)]
pub struct InputCompleter {
    walker: Walker,
    command: CommandCompleter,
}

impl InputCompleter {
    pub fn new(cwd: PathBuf, command_manager: Arc<ForgeCommandManager>) -> Self {
        let walker = Walker::max_all()
            .cwd(cwd)
            .skip_binary(true)
            .max_files(SHOW_MAX_COMPLETIONS);
        Self { walker, command: CommandCompleter::new(command_manager) }
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
            let term = query.term.to_lowercase();
            let files = self
                .walker
                .find_files(Some(move |path: &Path| {
                    path.file_name().is_none_or(|file_name| {
                        file_name.to_string_lossy().to_lowercase().contains(&term)
                    })
                }))
                .unwrap_or_default();

            files
                .into_iter()
                .filter(|file| !file.is_dir())
                .filter_map(|file| {
                    if let Some(file_name) = file.file_name.as_ref() {
                        let file_name_lower = file_name.to_lowercase();
                        let query_lower = query.term.to_lowercase();
                        if file_name_lower.contains(&query_lower) {
                            let path_md_fmt = format!("[{}]", file.path);
                            Some(Suggestion {
                                description: None,
                                value: path_md_fmt,
                                style: None,
                                extra: None,
                                span: query.span,
                                append_whitespace: true,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            vec![]
        }
    }
}
