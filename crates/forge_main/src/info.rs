use std::cmp::max;
use std::fmt;
use std::path::{Path, PathBuf};

use colored::Colorize;
use forge_api::{Environment, Usage};
use forge_tracker::VERSION;

use crate::model::ForgeCommandManager;
use crate::state::UIState;

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

impl From<&Usage> for Info {
    fn from(usage: &Usage) -> Self {
        let mut info = Info::new();
        let estimated = usage.estimated_tokens.unwrap_or(0);

        if estimated > usage.prompt_tokens {
            info = info.add_key_value("Prompt", format!("~{estimated}"));
        } else {
            info = info.add_key_value("Prompt", usage.prompt_tokens)
        }

        info.add_title("Usage".to_string())
            .add_key_value(
                "Prompt",
                max(usage.prompt_tokens, usage.estimated_tokens.unwrap_or(0)),
            )
            .add_key_value("Completion", usage.completion_tokens)
            .add_key_value("Total", usage.total_tokens)
    }
}

impl From<&Environment> for Info {
    fn from(env: &Environment) -> Self {
        // Get the current git branch
        let branch_info = match get_git_branch() {
            Some(branch) => branch,
            None => "(not in a git repository)".to_string(),
        };

        Info::new()
            .add_title("Environment")
            .add_key_value("Version", VERSION)
            .add_key_value(
                "Working Directory",
                format_path_zsh_style(&env.home, &env.cwd),
            )
            .add_key_value("Shell", &env.shell)
            .add_key_value("Git Branch", branch_info)
            .add_title("Paths")
            .add_key_value("Logs", format_path_zsh_style(&env.home, &env.log_path()))
            .add_key_value(
                "History",
                format_path_zsh_style(&env.home, &env.history_path()),
            )
            .add_key_value(
                "Checkpoints",
                format_path_zsh_style(&env.home, &env.snapshot_path()),
            )
    }
}

impl From<&UIState> for Info {
    fn from(value: &UIState) -> Self {
        let mut info = Info::new().add_title("Model");

        if let Some(model) = &value.model {
            info = info.add_key_value("Current", model);
        }

        info = info
            .add_key_value("Prompt Tokens", value.usage.prompt_tokens)
            .add_key_value("Completion Tokens", value.usage.completion_tokens)
            .add_key_value("Total Reported", value.usage.total_tokens);

        // Add estimated tokens if available
        if let Some(estimated) = value.usage.estimated_tokens {
            info = info.add_key_value("Total Estimated", estimated);
        }

        info
    }
}

impl fmt::Display for Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for section in &self.sections {
            match section {
                Section::Title(title) => {
                    writeln!(f)?;
                    writeln!(f, "{}", title.to_uppercase().bold().dimmed())?
                }
                Section::Items(key, value) => {
                    if let Some(value) = value {
                        writeln!(f, "{}: {}", key.bright_cyan().bold(), value)?;
                    } else {
                        writeln!(f, "{key}")?;
                    }
                }
            }
        }
        Ok(())
    }
}
/// Formats a path in zsh style, replacing home directory with ~
fn format_path_zsh_style(home: &Option<PathBuf>, path: &Path) -> String {
    if let Some(home) = home {
        if let Ok(rel_path) = path.strip_prefix(home) {
            return format!("~/{}", rel_path.display());
        }
    }
    path.display().to_string()
}

/// Gets the current git branch name if available
fn get_git_branch() -> Option<String> {
    // First check if we're in a git repository
    let git_check = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .ok()?;

    if !git_check.status.success() || git_check.stdout.is_empty() {
        return None;
    }

    // If we are in a git repo, get the branch
    let output = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}

/// Create an info instance for available commands from a ForgeCommandManager
impl From<&ForgeCommandManager> for Info {
    fn from(command_manager: &ForgeCommandManager) -> Self {
        let mut info = Info::new().add_title("Commands");

        for command in command_manager.list() {
            info = info.add_key_value(command.name, command.description);
        }

        info = info
            .add_title("Keyboard Shortcuts")
            .add_key_value("<CTRL+C>", "Interrupt current operation")
            .add_key_value("<CTRL+D>", "Quit Forge interactive shell")
            .add_key_value("<OPT+ENTER>", "Insert new line (multiline input)");

        info
    }
}
