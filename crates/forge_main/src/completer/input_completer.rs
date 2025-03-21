use std::path::PathBuf;
use std::sync::Arc;

use forge_walker::Walker;
use nu_ansi_term::{Color, Style};
use reedline::{Completer, Suggestion};

use crate::completer::search_term::SearchTerm;
use crate::completer::CommandCompleter;
use crate::model::ForgeCommandManager;

#[derive(Clone)]
pub struct InputCompleter {
    walker: Walker,
    command: CommandCompleter,
}

impl InputCompleter {
    pub fn new(cwd: PathBuf, command_manager: Arc<ForgeCommandManager>) -> Self {
        let walker = Walker::max_all().cwd(cwd).skip_binary(true);
        Self { walker, command: CommandCompleter::new(command_manager) }
    }

    fn create_suggestion(path: String, name: &str, query: &str, span: reedline::Span) -> Suggestion {
        let name_lower = name.to_lowercase();
        let query_lower = query.to_lowercase();
        
        // Simple highlight function that works for both prefix and substring matches
        let description = if query.is_empty() {
            name.to_string()
        } else if let Some(idx) = name_lower.find(&query_lower) {
            let style = Style::new().on(Color::Green).fg(Color::Black).bold();
            let end = idx + query.len();
            format!(
                "{}{}{}",
                &name[..idx],
                style.paint(&name[idx..end]),
                &name[end..]
            )
        } else {
            name.to_string()
        };

        Suggestion {
            value: path,
            description: Some(description),
            style: None,
            extra: Some(vec![
                if name_lower.starts_with(&query_lower) { "prefix" } else { "substring" }.to_string()
            ]),
            span,
            append_whitespace: true,
        }
    }
}

impl Completer for InputCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        // Handle command completion
        if line.starts_with('/') && !CommandCompleter.complete(line, pos).is_empty() {
            return CommandCompleter.complete(line, pos);
        }

        // Handle file completion
        let search_term = SearchTerm::new(line, pos);
        let Some(query) = search_term.process() else {
            return vec![];
        };

        let Ok(files) = self.walker.get_blocking() else {
            return vec![];
        };

        files
            .into_iter()
            .filter(|file| !file.is_dir())
            .filter_map(|file| {
                let name = file.file_name.as_ref()?;
                let matches = name.to_lowercase().contains(&query.term.to_lowercase());
                matches.then(|| Self::create_suggestion(file.path, name, query.term, query.span))
            })
            .collect()
    }
}
