use std::path::PathBuf;

use forge_walker::Walker;
use nu_ansi_term::{Color, Style};
use reedline::{Completer, Suggestion};

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

    fn create_styled_suggestion(
        file_path: String,
        file_name: &str,
        query: &str,
        span: reedline::Span,
        is_prefix: bool,
    ) -> Suggestion {
        // Create a style that highlights the matched portion with background color
        let highlight_style = Style::new().on(Color::Green).fg(Color::Black).bold();

        // Split the path into directory and filename
        let path_parts: Vec<&str> = file_path.rsplitn(2, '/').collect();
        let (file_part, _dir_part) = match path_parts.as_slice() {
            [file, dir] => (*file, Some(*dir)),
            [file] => (*file, None),
            _ => (file_name, None),
        };

        let description = if is_prefix {
            // For prefix matches, highlight just the matching prefix
            let query_len = query.len();
            if query_len > 0 {
                format!(
                    "{}{}",
                    highlight_style.paint(&file_part[..query_len]),
                    &file_part[query_len..]
                )
            } else {
                file_part.to_string()
            }
        } else {
            // For substring matches, find and highlight the matching portion
            let file_part_lower = file_part.to_lowercase();
            let query_lower = query.to_lowercase();

            if let Some(match_idx) = file_part_lower.find(&query_lower) {
                let match_end = match_idx + query.len();
                format!(
                    "{}{}{}",
                    &file_part[..match_idx],
                    highlight_style.paint(&file_part[match_idx..match_end]),
                    &file_part[match_end..]
                )
            } else {
                // Fallback case - shouldn't happen due to our filtering
                file_part.to_string()
            }
        };

        Suggestion {
            value: file_path,
            description: Some(description),
            style: None, // We're using the description for styling instead
            extra: Some(vec![
                if is_prefix { "prefix" } else { "substring" }.to_string()
            ]),
            span,
            append_whitespace: true,
        }
    }
}

impl Completer for InputCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        // First try command completion if line starts with '/'
        if line.starts_with("/") {
            let result = CommandCompleter.complete(line, pos);
            if !result.is_empty() {
                return result;
            }
        }

        // Then try file completion for any word
        if let Some(query) = SearchTerm::new(line, pos).process() {
            let files = self.walker.get_blocking().unwrap_or_default();
            files
                .into_iter()
                .filter(|file| !file.is_dir()) // Skip directories
                .filter_map(|file| {
                    if let Some(file_name) = file.file_name.as_ref() {
                        let file_name_lower = file_name.to_lowercase();
                        let query_lower = query.term.to_lowercase();

                        // First try prefix match (higher priority)
                        let is_prefix_match = file_name_lower.starts_with(&query_lower);
                        // Then try substring match (lower priority)
                        let is_substring_match =
                            !is_prefix_match && file_name_lower.contains(&query_lower);

                        if is_prefix_match || is_substring_match {
                            Some(Self::create_styled_suggestion(
                                file.path,
                                file_name,
                                query.term,
                                query.span,
                                is_prefix_match,
                            ))
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
