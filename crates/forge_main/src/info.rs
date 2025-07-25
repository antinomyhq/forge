use std::fmt;
use std::path::Path;

use colored::Colorize;
use forge_api::{Environment, LoginInfo};
use forge_tracker::VERSION;

use crate::model::ForgeCommandManager;
use crate::state::UIState;

#[derive(Debug, PartialEq)]
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

impl From<&Environment> for Info {
    fn from(env: &Environment) -> Self {
        // Get the current git branch
        let branch_info = match get_git_branch() {
            Some(branch) => branch,
            None => "(not in a git repository)".to_string(),
        };

        let mut info = Info::new()
            .add_title("Environment")
            .add_key_value("Version", VERSION)
            .add_key_value("Working Directory", format_path_for_display(env, &env.cwd))
            .add_key_value("Shell", &env.shell)
            .add_key_value("Git Branch", branch_info)
            .add_title("Paths");

        // Only show logs path if the directory exists
        let log_path = env.log_path();
        if log_path.exists() {
            info = info.add_key_value("Logs", format_path_for_display(env, &log_path));
        }

        info = info
            .add_key_value("History", format_path_for_display(env, &env.history_path()))
            .add_key_value(
                "Checkpoints",
                format_path_for_display(env, &env.snapshot_path()),
            );

        info
    }
}

impl From<&UIState> for Info {
    fn from(value: &UIState) -> Self {
        let mut info = Info::new().add_title("Model");

        if let Some(model) = &value.model {
            info = info.add_key_value("Current", model);
        }

        if let Some(provider) = &value.provider {
            info = info.add_key_value("Provider (URL)", provider.to_base_url());
        }

        let usage = &value.usage;

        info = info
            .add_title("Usage".to_string())
            .add_key_value("Prompt", &usage.prompt_tokens)
            .add_key_value("Completion", &usage.completion_tokens)
            .add_key_value("Total", &usage.total_tokens)
            .add_key_value("Cached Tokens", &usage.cached_tokens);

        if let Some(cost) = usage.cost {
            info = info.add_key_value("Cost", format!("${cost:.4}"));
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

/// Formats a path for display, using %USERPROFILE% on Windows and tilde
/// notation on Unix
fn format_path_for_display(env: &Environment, path: &Path) -> String {
    // Check if path is under home directory first
    if let Some(home) = &env.home
        && let Ok(rel_path) = path.strip_prefix(home)
    {
        // Format based on OS
        return if env.os == "windows" {
            format!(
                "%USERPROFILE%{}{}",
                std::path::MAIN_SEPARATOR,
                rel_path.display()
            )
        } else {
            format!("~/{}", rel_path.display())
        }
    }

    // Fall back to absolute path if not under home directory
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

impl From<&LoginInfo> for Info {
    fn from(login_info: &LoginInfo) -> Self {
        let mut info = Info::new().add_title("User");

        if let Some(email) = &login_info.email {
            info = info.add_key_value("Login", email);
        }

        info = info.add_key_value("Key", truncate_key(&login_info.api_key_masked));

        info
    }
}

fn truncate_key(key: &str) -> String {
    if key.len() <= 20 {
        key.to_string()
    } else {
        format!("{}...{}", &key[..=12], &key[key.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_api::{Environment, LoginInfo};
    use pretty_assertions::assert_eq;

    use crate::info::Info;

    #[test]
    fn test_login_info_display() {
        let fixture = LoginInfo {
            api_key: "test-key".to_string(),
            api_key_name: "Test Key".to_string(),
            api_key_masked: "sk-fg-v1-abcd...1234".to_string(),
            email: Some("test@example.com".to_string()),
            name: Some("Test User".to_string()),
            auth_provider_id: Some("provider-id".to_string()),
        };

        let actual = Info::from(&fixture);

        let expected = Info::new()
            .add_title("User")
            .add_key_value("Login", "test@example.com")
            .add_key_value("Key", "sk-fg-v1-abcd...1234");

        assert_eq!(actual.sections, expected.sections);
    }

    #[test]
    fn test_login_info_display_no_name() {
        let fixture = LoginInfo {
            api_key: "test-key".to_string(),
            api_key_name: "Test Key".to_string(),
            api_key_masked: "sk-fg-v1-abcd...1234".to_string(),
            email: Some("test@example.com".to_string()),
            name: None,
            auth_provider_id: Some("provider-id".to_string()),
        };

        let actual = Info::from(&fixture);

        let expected = Info::new()
            .add_title("User")
            .add_key_value("Login", "test@example.com")
            .add_key_value("Key", "sk-fg-v1-abcd...1234");

        assert_eq!(actual.sections, expected.sections);
    }

    #[test]
    fn test_format_path_for_display_unix() {
        // Test Unix-style path formatting with tilde notation
        let fixture = Environment {
            os: "linux".to_string(),
            pid: 1234,
            cwd: PathBuf::from("/home/user/project"),
            home: Some(PathBuf::from("/home/user")),
            shell: "bash".to_string(),
            base_path: PathBuf::from("/home/user/.forge"),
            forge_api_url: "https://api.forge.com".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 1000,
            fetch_truncation_limit: 10000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            max_read_size: 2000,
            http: Default::default(),
            max_file_size: 1000000,
        };
        let path = PathBuf::from("/home/user/forge/snapshots");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "~/forge/snapshots";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_windows() {
        // Test Windows-style path formatting with %USERPROFILE% notation
        let home_path = if cfg!(windows) {
            PathBuf::from("C:\\Users\\User")
        } else {
            PathBuf::from("C:/Users/User")
        };

        let test_path = if cfg!(windows) {
            PathBuf::from("C:\\Users\\User\\forge\\snapshots")
        } else {
            PathBuf::from("C:/Users/User/forge/snapshots")
        };

        let fixture = Environment {
            os: "windows".to_string(),
            pid: 1234,
            cwd: PathBuf::from("C:/Users/User/project"),
            home: Some(home_path),
            shell: "cmd".to_string(),
            base_path: PathBuf::from("C:/Users/User/.forge"),
            forge_api_url: "https://api.forge.com".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 1000,
            fetch_truncation_limit: 10000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            max_read_size: 2000,
            http: Default::default(),
            max_file_size: 1000000,
        };

        let actual = super::format_path_for_display(&fixture, &test_path);
        let expected = if cfg!(windows) {
            "%USERPROFILE%\\forge\\snapshots"
        } else {
            "%USERPROFILE%/forge/snapshots"
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_absolute_path() {
        // Test path outside home directory
        let fixture = Environment {
            os: "linux".to_string(),
            pid: 1234,
            cwd: PathBuf::from("/home/user/project"),
            home: Some(PathBuf::from("/home/user")),
            shell: "bash".to_string(),
            base_path: PathBuf::from("/home/user/.forge"),
            forge_api_url: "https://api.forge.com".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 1000,
            fetch_truncation_limit: 10000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            max_read_size: 2000,
            http: Default::default(),
            max_file_size: 1000000,
        };
        let path = PathBuf::from("/var/log/forge");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "/var/log/forge";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_macos() {
        // Test macOS-style path formatting with tilde notation
        let fixture = Environment {
            os: "macos".to_string(),
            pid: 1234,
            cwd: PathBuf::from("/Users/user/project"),
            home: Some(PathBuf::from("/Users/user")),
            shell: "zsh".to_string(),
            base_path: PathBuf::from("/Users/user/.forge"),
            forge_api_url: "https://api.forge.com".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 1000,
            fetch_truncation_limit: 10000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            max_read_size: 2000,
            http: Default::default(),
            max_file_size: 1000000,
        };
        let path = PathBuf::from("/Users/user/forge/snapshots");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "~/forge/snapshots";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_windows_userprofile() {
        // Test Windows with USERPROFILE-style path
        let home_path = if cfg!(windows) {
            PathBuf::from("C:\\Users\\JohnDoe")
        } else {
            PathBuf::from("C:/Users/JohnDoe")
        };

        let test_path = if cfg!(windows) {
            PathBuf::from("C:\\Users\\JohnDoe\\forge\\logs")
        } else {
            PathBuf::from("C:/Users/JohnDoe/forge/logs")
        };

        let fixture = Environment {
            os: "windows".to_string(),
            pid: 1234,
            cwd: PathBuf::from("C:/Users/JohnDoe/project"),
            home: Some(home_path),
            shell: "powershell".to_string(),
            base_path: PathBuf::from("C:/Users/JohnDoe/.forge"),
            forge_api_url: "https://api.forge.com".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 1000,
            fetch_truncation_limit: 10000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            max_read_size: 2000,
            http: Default::default(),
            max_file_size: 1000000,
        };

        let actual = super::format_path_for_display(&fixture, &test_path);
        let expected = if cfg!(windows) {
            "%USERPROFILE%\\forge\\logs"
        } else {
            "%USERPROFILE%/forge/logs"
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_unix_home() {
        // Test Unix with HOME-style path
        let fixture = Environment {
            os: "linux".to_string(),
            pid: 1234,
            cwd: PathBuf::from("/home/jane/project"),
            home: Some(PathBuf::from("/home/jane")),
            shell: "bash".to_string(),
            base_path: PathBuf::from("/home/jane/.forge"),
            forge_api_url: "https://api.forge.com".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 1000,
            fetch_truncation_limit: 10000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            max_read_size: 2000,
            http: Default::default(),
            max_file_size: 1000000,
        };
        let path = PathBuf::from("/home/jane/.config/forge");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "~/.config/forge";
        assert_eq!(actual, expected);
    }
}
