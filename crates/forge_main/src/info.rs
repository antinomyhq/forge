use std::fmt;

use colored::Colorize;
use forge_api::{Environment, Usage};
use forge_tracker::VERSION;

pub enum Section {
    Title(String),
    Items(String, String),
}

pub struct Info {
    sections: Vec<Section>,
}

impl Info {
    pub fn new() -> Self {
        Info { sections: Vec::new() }
    }

    pub fn add_title(mut self, title: impl ToString) -> Self {
        self.sections.push(Section::Title(title.to_string()));
        self
    }

    pub fn add_item(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.sections
            .push(Section::Items(key.to_string(), value.to_string()));
        self
    }

    pub fn extend(mut self, other: Info) -> Self {
        self.sections.extend(other.sections);
        self
    }
}

impl From<&Usage> for Info {
    fn from(usage: &Usage) -> Self {
        Info::new()
            .add_title("Usage".to_string())
            .add_item("Prompt", usage.prompt_tokens)
            .add_item("Completion", usage.completion_tokens)
            .add_item("Total", usage.total_tokens)
    }
}

impl From<&Environment> for Info {
    fn from(env: &Environment) -> Self {
        Info::new()
            .add_title("Environment")
            .add_item("Version", VERSION)
            .add_item("OS", &env.os)
            .add_item("PID", env.pid)
            .add_item("Working Directory", env.cwd.display())
            .add_item("Shell", &env.shell)
            .add_title("Paths")
            .add_item("Config", env.base_path.display())
            .add_item("Logs", env.log_path().display())
            .add_item("Database", env.db_path().display())
            .add_item("History", env.history_path().display())
    }
}

impl fmt::Display for Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for section in &self.sections {
            match section {
                Section::Title(title) => {
                    writeln!(f)?;
                    writeln!(f, "{}", title.bold().bright_yellow())?
                }
                Section::Items(key, value) => {
                    writeln!(f, "{}: {}", key, value.dimmed())?;
                }
            }
        }
        Ok(())
    }
}

// The display_info function has been removed and its implementation will be
// inlined in the caller
