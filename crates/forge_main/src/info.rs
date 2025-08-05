use std::fmt;
use std::path::Path;
use std::time::Duration;

use colored::Colorize;
use forge_api::{Environment, LoginInfo, UserUsage};
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
            if let Some(api_key) = &provider.key() {
                info = info.add_key_value("API Key", truncate_key(api_key));
            }
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

/// Formats a path for display, using actual home directory on Windows and tilde
/// notation on Unix, with proper quoting for paths containing spaces
fn format_path_for_display(env: &Environment, path: &Path) -> String {
    // Check if path is under home directory first
    if let Some(home) = &env.home
        && let Ok(rel_path) = path.strip_prefix(home)
    {
        // Format based on OS
        return if env.os == "windows" {
            // Use actual home path with proper quoting for Windows to work in both cmd and
            // PowerShell
            let home_path = home.display().to_string();
            let full_path = format!(
                "{}{}{}",
                home_path,
                std::path::MAIN_SEPARATOR,
                rel_path.display()
            );
            if full_path.contains(' ') {
                format!("\"{full_path}\"")
            } else {
                full_path
            }
        } else {
            format!("~/{}", rel_path.display())
        };
    }

    // Fall back to absolute path if not under home directory
    // Quote paths on Windows if they contain spaces
    let path_str = path.display().to_string();
    if env.os == "windows" && path_str.contains(' ') {
        format!("\"{path_str}\"")
    } else {
        path_str
    }
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
        let mut info = Info::new().add_title("Account");

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

impl From<&UserUsage> for Info {
    fn from(user_usage: &UserUsage) -> Self {
        let usage = &user_usage.usage;
        let plan = &user_usage.plan;

        // Create progress bar for usage visualization
        let progress_bar = create_progress_bar(usage.current, usage.limit, 20);

        let mut info = Info::new()
            .add_title(format!("{} Quota", plan.r#type.to_uppercase()))
            .add_key_value(
                "Usage",
                format!(
                    "{} / {} [{} Remaining]",
                    usage.current, usage.limit, usage.remaining
                ),
            );

        // Add reset information if available
        if let Some(reset_in) = usage.reset_in {
            let reset_info = format_reset_time(reset_in);
            info = info.add_key_value("Resets in", reset_info);
        }

        info.add_key_value("Progress", progress_bar)
    }
}

pub fn create_progress_bar(current: u32, limit: u32, width: usize) -> String {
    if limit == 0 {
        return "N/A".to_string();
    }

    let percentage = (current as f64 / limit as f64 * 100.0).min(100.0);
    let filled_chars = ((current as f64 / limit as f64) * width as f64).round() as usize;
    let filled_chars = filled_chars.min(width);
    let empty_chars = width - filled_chars;

    // Option 1: Unicode block characters (most visually appealing)
    format!(
        "▐{}{} {:.1}%",
        "█".repeat(filled_chars),
        "░".repeat(empty_chars),
        percentage
    )
}

pub fn format_reset_time(seconds: u64) -> String {
    if seconds == 0 {
        return "now".to_string();
    }
    humantime::format_duration(Duration::from_secs(seconds)).to_string()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_api::Environment;
    use pretty_assertions::assert_eq;

    // Helper to create minimal test environment
    fn create_env(os: &str, home: Option<&str>) -> Environment {
        Environment {
            os: os.to_string(),
            home: home.map(PathBuf::from),
            // Minimal required fields with defaults
            pid: 1,
            cwd: PathBuf::from("/"),
            shell: "bash".to_string(),
            base_path: PathBuf::from("/tmp"),
            forge_api_url: "http://localhost".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 100,
            max_search_result_bytes: 100, // 0.25 MB
            fetch_truncation_limit: 1000,
            stdout_max_prefix_length: 10,
            stdout_max_suffix_length: 10,
            stdout_max_line_length: 2000,
            max_read_size: 100,
            http: Default::default(),
            max_file_size: 1000,
        }
    }

    #[test]
    fn test_format_path_for_display_unix_home() {
        let fixture = create_env("linux", Some("/home/user"));
        let path = PathBuf::from("/home/user/project");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "~/project";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_windows_home() {
        let fixture = create_env("windows", Some("C:\\Users\\User"));
        let path = PathBuf::from("C:\\Users\\User\\project");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = if cfg!(windows) {
            "C:\\Users\\User\\project"
        } else {
            "C:\\Users\\User\\project"
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_windows_home_with_spaces() {
        let fixture = create_env("windows", Some("C:\\Users\\User Name"));
        let path = PathBuf::from("C:\\Users\\User Name\\project");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = if cfg!(windows) {
            "\"C:\\Users\\User Name\\project\""
        } else {
            "\"C:\\Users\\User Name\\project\""
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_absolute() {
        let fixture = create_env("linux", Some("/home/user"));
        let path = PathBuf::from("/var/log/app");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "/var/log/app";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_absolute_windows_with_spaces() {
        let fixture = create_env("windows", Some("C:/Users/User"));
        let path = PathBuf::from("C:/Program Files/App");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "\"C:/Program Files/App\"";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_no_home() {
        let fixture = create_env("linux", None);
        let path = PathBuf::from("/home/user/project");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "/home/user/project";
        assert_eq!(actual, expected);
    }
    #[test]
    fn test_create_progress_bar() {
        // Test normal case - 70% of 20 = 14 filled, 6 empty
        let actual = super::create_progress_bar(70, 100, 20);
        let expected = "▐██████████████░░░░░░ 70.0%";
        assert_eq!(actual, expected);

        // Test 100% case
        let actual = super::create_progress_bar(100, 100, 20);
        let expected = "▐████████████████████ 100.0%";
        assert_eq!(actual, expected);

        // Test 0% case
        let actual = super::create_progress_bar(0, 100, 20);
        let expected = "▐░░░░░░░░░░░░░░░░░░░░ 0.0%";
        assert_eq!(actual, expected);

        // Test zero limit case
        let actual = super::create_progress_bar(50, 0, 20);
        let expected = "N/A";
        assert_eq!(actual, expected);

        // Test over 100% case (should cap at 100%)
        let actual = super::create_progress_bar(150, 100, 20);
        let expected = "▐████████████████████ 100.0%";
        assert_eq!(actual, expected);
    }
    #[test]
    fn test_format_reset_time_hours_and_minutes() {
        let actual = super::format_reset_time(3661); // 1 hour, 1 minute, 1 second
        let expected = "1h 1m 1s";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_reset_time_hours_only() {
        let actual = super::format_reset_time(3600); // exactly 1 hour
        let expected = "1h";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_reset_time_minutes_and_seconds() {
        let actual = super::format_reset_time(125); // 2 minutes, 5 seconds
        let expected = "2m 5s";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_reset_time_minutes_only() {
        let actual = super::format_reset_time(120); // exactly 2 minutes
        let expected = "2m";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_reset_time_seconds_only() {
        let actual = super::format_reset_time(45); // 45 seconds
        let expected = "45s";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_reset_time_zero() {
        let actual = super::format_reset_time(0);
        let expected = "now";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_reset_time_large_value() {
        let actual = super::format_reset_time(7265); // 2 hours, 1 minute, 5 seconds
        let expected = "2h 1m 5s";
        assert_eq!(actual, expected);
    }
}
