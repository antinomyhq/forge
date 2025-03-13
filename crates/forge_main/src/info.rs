use std::fmt;

use colored::Colorize;
use forge_api::{Environment, Usage};
use forge_tracker::VERSION;

pub enum Section {
    Title(String),
    Items(String, Option<String>),
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

    pub fn add_key(self, key: impl ToString) -> Self {
        self.add_item(key, None::<String>)
    }

    pub fn add_key_value(self, key: impl ToString, value: impl ToString) -> Self {
        self.add_item(key, Some(value))
    }

    fn add_item(mut self, key: impl ToString, value: Option<impl ToString>) -> Self {
        self.sections.push(Section::Items(
            key.to_string(),
            value.map(|a| a.to_string()),
        ));
        self
    }

    pub fn extend(mut self, other: Info) -> Self {
        self.sections.extend(other.sections);
        self
    }
}

pub struct UsageInfo<'a> {
    usage: &'a Usage,
    total_snapshots: usize,
}

impl<'a> UsageInfo<'a> {
    pub fn new(usage: &'a Usage, total_snapshots: usize) -> Self {
        Self { usage, total_snapshots }
    }
}

impl From<UsageInfo<'_>> for Info {
    fn from(usage_info: UsageInfo) -> Self {
        Info::new()
            .add_title("Usage".to_string())
            .add_key_value("Prompt", usage_info.usage.prompt_tokens)
            .add_key_value("Completion", usage_info.usage.completion_tokens)
            .add_key_value("Total", usage_info.usage.total_tokens)
            .add_item("Total Snapshots", usage_info.total_snapshots)
    }
}

impl From<&Environment> for Info {
    fn from(env: &Environment) -> Self {
        Info::new()
            .add_title("Environment")
            .add_key_value("Version", VERSION)
            .add_key_value("OS", &env.os)
            .add_key_value("PID", env.pid)
            .add_key_value("Working Directory", env.cwd.display())
            .add_key_value("Shell", &env.shell)
            .add_title("Paths")
            .add_key_value("Config", env.base_path.display())
            .add_key_value("Logs", env.log_path().display())
            .add_key_value("Database", env.db_path().display())
            .add_key_value("History", env.history_path().display())
            .add_item("Snapshot", env.snapshot_path().display())
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
                    if let Some(value) = value {
                        writeln!(f, "{}: {}", key, value.dimmed())?;
                    } else {
                        writeln!(f, "{}", key)?;
                    }
                }
            }
        }
        Ok(())
    }
}

// The display_info function has been removed and its implementation will be
// inlined in the caller
