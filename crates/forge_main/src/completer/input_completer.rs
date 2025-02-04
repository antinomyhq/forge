use std::path::PathBuf;

use forge_walker::Walker;
use reedline::{Completer, Suggestion};
use tracing::info;

use crate::completer::search_term::SearchTerm;
use crate::completer::CommandCompleter;

#[derive(Clone)]
pub struct InputCompleter {
    walker: Walker,
}

impl InputCompleter {
    pub fn new(cwd: PathBuf) -> Self {
        let walker = Walker::max_all().cwd(cwd).skip_binary(true);
        Self { walker }
    }
}

impl Completer for InputCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        info!("Completing line: '{}' pos: {}", line, pos);

        if line.starts_with("/") {
            // if the line starts with '/' it's probably a command, so we delegate to the
            // command completer.
            let result = CommandCompleter.complete(line, pos);
            if !result.is_empty() {
                return result;
            }
        }

        if let Some(query) = SearchTerm::new(line, pos).process() {
            info!("Search term: {:?}", query);

            let files = self.walker.get_blocking().unwrap_or_default();
            files
                .into_iter()
                .filter(|file| !file.is_dir())
                .filter_map(|file| {
                    if let Some(file_name) = file.file_name.as_ref() {
                        if file_name.contains(query.term) {
                            Some(Suggestion {
                                value: file.path,
                                description: None,
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
