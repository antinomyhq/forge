use std::fmt;
use std::path::Path;
use std::time::Duration;

use colored::Colorize;
use forge_api::{Conversation, Environment, LoginInfo, Metrics, Usage, UserUsage};
use forge_app::utils::truncate_key;
use forge_tracker::VERSION;
use num_format::{Locale, ToFormattedString};

use crate::model::ForgeCommandManager;

#[derive(Debug, PartialEq)]
pub enum Section {
    Title(String),
    Items(String, Option<String>, Option<String>), // key, value, subtitle
}

#[derive(Default)]
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
        self.add_item(key, None::<String>, None::<String>)
    }

    pub fn add_key_value(self, key: impl ToString, value: impl ToString) -> Self {
        self.add_item(key, Some(value), None::<String>)
    }

    pub fn add_key_value_subtitle(
        self,
        key: impl ToString,
        value: impl ToString,
        subtitle: impl ToString,
    ) -> Self {
        self.add_item(key, Some(value), Some(subtitle))
    }

    fn add_item(
        mut self,
        key: impl ToString,
        value: Option<impl ToString>,
        subtitle: Option<impl ToString>,
    ) -> Self {
        self.sections.push(Section::Items(
            key.to_string(),
            value.map(|a| a.to_string()),
            subtitle.map(|a| a.to_string()),
        ));
        self
    }

    pub fn extend(mut self, other: impl Into<Info>) -> Self {
        self.sections.extend(other.into().sections);
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

        let mut info = Info::new().add_title("PATHS");

        // Only show logs path if the directory exists
        let log_path = env.log_path();
        if log_path.exists() {
            info = info.add_key_value("Logs", format_path_for_display(env, &log_path));
        }

        let agent_path = env.agent_path();
        info = info.add_key_value("Agents", format_path_for_display(env, &agent_path));

        info = info
            .add_key_value("History", format_path_for_display(env, &env.history_path()))
            .add_key_value(
                "Checkpoints",
                format_path_for_display(env, &env.snapshot_path()),
            )
            .add_key_value(
                "Policies",
                format_path_for_display(env, &env.permissions_path()),
            )
            .add_title("ENVIRONMENT")
            .add_key_value("Version", VERSION)
            .add_key_value("Working Directory", format_path_for_display(env, &env.cwd))
            .add_key_value("Shell", &env.shell)
            .add_key_value("Git Branch", branch_info);

        info
    }
}

impl From<&Metrics> for Info {
    fn from(metrics: &Metrics) -> Self {
        let mut info = Info::new();
        if let Some(duration) = metrics.duration()
            && duration.as_secs() > 0
        {
            let duration =
                humantime::format_duration(Duration::from_secs(duration.as_secs())).to_string();
            info = info.add_title(format!("TASK COMPLETED [in {duration}]"));
        } else {
            info = info.add_title("TASK COMPLETED".to_string())
        }

        // Add file changes section inspired by the example format
        if metrics.files_changed.is_empty() {
            info = info.add_key("[No Changes Produced]");
        } else {
            // First, calculate the maximum filename length for proper alignment
            let max_filename_len = metrics
                .files_changed
                .keys()
                .map(|path| {
                    std::path::Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(path)
                        .len()
                })
                .max()
                .unwrap_or(0);

            // Add each file with its changes, padded for alignment
            for (path, file_metrics) in &metrics.files_changed {
                // Extract just the filename from the path
                let filename = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path);

                let changes = format!(
                    "−{} +{}",
                    file_metrics.lines_removed, file_metrics.lines_added
                );

                // Pad filename to max length for proper alignment
                let padded_filename = format!("⦿ {filename:<max_filename_len$}");
                info = info.add_key_value(padded_filename, changes);
            }
        }

        info
    }
}

impl From<&Usage> for Info {
    fn from(value: &Usage) -> Self {
        let cache_percentage = calculate_cache_percentage(value);
        let cached_display = if cache_percentage > 0 {
            format!(
                "{} [{}%]",
                value.cached_tokens.to_formatted_string(&Locale::en),
                cache_percentage
            )
        } else {
            value.cached_tokens.to_formatted_string(&Locale::en)
        };

        let mut usage_info = Info::new()
            .add_title("TOKEN USAGE")
            .add_key_value(
                "Input Tokens",
                value.prompt_tokens.to_formatted_string(&Locale::en),
            )
            .add_key_value("Cached Tokens", cached_display)
            .add_key_value(
                "Output Tokens",
                value.completion_tokens.to_formatted_string(&Locale::en),
            );

        if let Some(cost) = value.cost.as_ref() {
            usage_info = usage_info.add_key_value("Cost", format!("${cost:.4}"));
        }
        usage_info
    }
}

fn calculate_cache_percentage(usage: &Usage) -> u8 {
    let total = *usage.prompt_tokens; // Use prompt tokens as the base for cache percentage
    let cached = *usage.cached_tokens;
    if total == 0 {
        0
    } else {
        ((cached * 100) / total) as u8
    }
}

impl fmt::Display for Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for section in &self.sections {
            match section {
                Section::Title(title) => {
                    writeln!(f)?;
                    writeln!(f, "{}", title.bold().dimmed())?
                }
                Section::Items(key, value, subtitle) => {
                    if let Some(value) = value {
                        if let Some(subtitle) = subtitle {
                            writeln!(
                                f,
                                "  {}: {} [{}]",
                                key.bright_cyan().bold(),
                                value,
                                subtitle.dimmed()
                            )?;
                        } else {
                            writeln!(f, "  {}: {}", key.bright_cyan().bold(), value)?;
                        }
                    } else {
                        writeln!(f, "  {key}")?;
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
        let mut info = Info::new().add_title("COMMANDS");

        for command in command_manager.list() {
            info = info.add_key_value(command.name, command.description);
        }

        info = info
            .add_title("KEYBOARD SHORTCUTS")
            .add_key_value("<CTRL+C>", "Interrupt current operation")
            .add_key_value("<CTRL+D>", "Quit Forge interactive shell")
            .add_key_value("<OPT+ENTER>", "Insert new line (multiline input)");

        info
    }
}
impl From<&LoginInfo> for Info {
    fn from(login_info: &LoginInfo) -> Self {
        let mut info = Info::new().add_title("ACCOUNT");

        if let Some(email) = &login_info.email {
            info = info.add_key_value("Login", email);
        }

        info = info.add_key_value("Key", truncate_key(&login_info.api_key_masked));

        info
    }
}

impl From<&UserUsage> for Info {
    fn from(user_usage: &UserUsage) -> Self {
        let usage = &user_usage.usage;
        let plan = &user_usage.plan;

        let mut info = Info::new().add_title("REQUEST QUOTA");

        if plan.is_upgradeable() {
            info = info.add_key_value(
                "Subscription",
                format!(
                    "{} [Upgrade https://app.forgecode.dev/app/billing]",
                    plan.r#type.to_uppercase()
                ),
            );
        } else {
            info = info.add_key_value("Subscription", plan.r#type.to_uppercase());
        }

        info = info.add_key_value(
            "Usage",
            format!(
                "{} / {} [{} Remaining]",
                usage.current, usage.limit, usage.remaining
            ),
        );

        // Create progress bar for usage visualization
        let progress_bar = create_progress_bar(usage.current, usage.limit, 20);

        // Add reset information if available
        if let Some(reset_in) = usage.reset_in {
            info = info.add_key_value("Resets in", format_reset_time(reset_in));
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

impl From<&Conversation> for Info {
    fn from(conversation: &Conversation) -> Self {
        let mut info = Info::new().add_title("CONVERSATION");

        info = info.add_key_value("ID", conversation.id.to_string());

        if let Some(title) = &conversation.title {
            info = info.add_key_value("Title", title);
        }

        // Insert metrics information
        if !conversation.metrics.files_changed.is_empty() {
            info = info.extend(&conversation.metrics);
        }

        // Insert token usage
        if let Some(usage) = conversation.context.as_ref().and_then(|c| c.usage.as_ref()) {
            info = info.extend(usage);
        }

        info
    }
}

impl Info {
    /// Converts Info into a vector of tuples for porcelain output
    ///
    /// Extracts all Items sections (skipping Titles) and returns them as either
    /// 2-column (key, value) or 3-column (key, value, subtitle) tuples
    /// depending on whether subtitles are present.
    pub fn to_rows(&self) -> InfoRows {
        let has_subtitles = self
            .sections
            .iter()
            .any(|section| matches!(section, Section::Items(_, _, Some(_))));

        if has_subtitles {
            let rows: Vec<(String, String, String)> = self
                .sections
                .iter()
                .filter_map(|section| match section {
                    Section::Title(_) => None,
                    Section::Items(key, value, subtitle) => Some((
                        key.clone(),
                        value.clone().unwrap_or_default(),
                        subtitle.clone().unwrap_or_default(),
                    )),
                })
                .collect();
            InfoRows::ThreeColumn(rows)
        } else {
            let rows: Vec<(String, String)> = self
                .sections
                .iter()
                .filter_map(|section| match section {
                    Section::Title(_) => None,
                    Section::Items(key, value, _) => {
                        Some((key.clone(), value.clone().unwrap_or_default()))
                    }
                })
                .collect();
            InfoRows::TwoColumn(rows)
        }
    }
}

/// Represents rows from Info that can be either 2 or 3 columns
#[derive(Debug, PartialEq)]
pub enum InfoRows {
    TwoColumn(Vec<(String, String)>),
    ThreeColumn(Vec<(String, String, String)>),
}

impl InfoRows {
    /// Formats the rows using format_columns regardless of column count
    pub fn format(self) {
        match self {
            InfoRows::TwoColumn(rows) => crate::cli_format::format_columns(rows),
            InfoRows::ThreeColumn(rows) => crate::cli_format::format_columns(rows),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;
    use forge_api::Environment;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::info::Info;

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
            tool_timeout: 300,
            http: Default::default(),
            max_file_size: 1000,
            auto_open_dump: false,
            custom_history_path: None,
            max_conversations: 100,
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
    fn test_format_path_for_display_no_home() {
        let fixture = create_env("linux", None);
        let path = PathBuf::from("/home/user/project");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "/home/user/project";
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
    #[test]
    fn test_metrics_info_display() {
        use forge_api::Metrics;

        let mut fixture = Metrics::new().with_time(Utc::now());
        fixture.record_file_operation("src/main.rs".to_string(), 12, 3);
        fixture.record_file_operation("src/agent/mod.rs".to_string(), 8, 2);
        fixture.record_file_operation("tests/integration/test_agent.rs".to_string(), 5, 0);

        let actual = super::Info::from(&fixture);
        let expected_display = actual.to_string();

        // Verify it contains the task completed section
        assert!(expected_display.contains("TASK COMPLETED"));

        // Verify it contains the files with bullet points
        assert!(expected_display.contains("⦿ main.rs"));
        assert!(expected_display.contains("−3 +12"));
        assert!(expected_display.contains("mod.rs"));
        assert!(expected_display.contains("−2 +8"));
        assert!(expected_display.contains("test_agent.rs"));
        assert!(expected_display.contains("−0 +5"));
    }

    #[test]
    fn test_conversation_info_display() {
        use chrono::Utc;
        use forge_api::ConversationId;

        use super::{Conversation, Metrics};

        let conversation_id = ConversationId::generate();
        let mut metrics = Metrics::new().with_time(Utc::now());
        metrics.record_file_operation("src/main.rs".to_string(), 5, 2);
        metrics.record_file_operation("tests/test.rs".to_string(), 3, 1);

        let fixture = Conversation {
            id: conversation_id,
            title: Some("Test Conversation".to_string()),
            context: None,
            metrics,
            metadata: forge_domain::MetaData::new(Utc::now()),
        };

        let actual = super::Info::from(&fixture);
        let expected_display = actual.to_string();

        // Verify it contains the conversation section
        assert!(expected_display.contains("CONVERSATION"));
        assert!(expected_display.contains("Test Conversation"));
        assert!(expected_display.contains(&conversation_id.to_string()));
    }

    #[test]
    fn test_conversation_info_display_untitled() {
        use chrono::Utc;
        use forge_api::ConversationId;

        use super::{Conversation, Metrics};

        let conversation_id = ConversationId::generate();
        let metrics = Metrics::new().with_time(Utc::now());

        let fixture = Conversation {
            id: conversation_id,
            title: None,
            context: None,
            metrics,
            metadata: forge_domain::MetaData::new(Utc::now()),
        };

        let actual = super::Info::from(&fixture);
        let expected_display = actual.to_string();

        // Verify it contains the conversation section with untitled
        assert!(expected_display.contains("CONVERSATION"));
        assert!(!expected_display.contains("Title:"));
        assert!(expected_display.contains(&conversation_id.to_string()));
    }

    #[test]
    fn test_to_rows_empty_info() {
        let fixture = Info::new();
        let actual = fixture.to_rows();
        let expected = InfoRows::TwoColumn(Vec::new());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_rows_only_titles() {
        let fixture = Info::new().add_title("TITLE1").add_title("TITLE2");
        let actual = fixture.to_rows();
        let expected = InfoRows::TwoColumn(Vec::new());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_rows_items_with_values() {
        let fixture = Info::new()
            .add_key_value("Key1", "Value1")
            .add_key_value("Key2", "Value2");
        let actual = fixture.to_rows();
        let expected = InfoRows::TwoColumn(vec![
            ("Key1".to_string(), "Value1".to_string()),
            ("Key2".to_string(), "Value2".to_string()),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_rows_items_without_values() {
        let fixture = Info::new().add_key("Key1").add_key("Key2");
        let actual = fixture.to_rows();
        let expected = InfoRows::TwoColumn(vec![
            ("Key1".to_string(), "".to_string()),
            ("Key2".to_string(), "".to_string()),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_rows_mixed_content() {
        let fixture = Info::new()
            .add_title("TITLE1")
            .add_key_value("Key1", "Value1")
            .add_title("TITLE2")
            .add_key("Key2")
            .add_key_value("Key3", "Value3");
        let actual = fixture.to_rows();
        let expected = InfoRows::TwoColumn(vec![
            ("Key1".to_string(), "Value1".to_string()),
            ("Key2".to_string(), "".to_string()),
            ("Key3".to_string(), "Value3".to_string()),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_rows_complex_usage_pattern() {
        // Simulate the Environment Info pattern
        let fixture = Info::new()
            .add_title("PATHS")
            .add_key_value("Logs", "/path/to/logs")
            .add_key_value("Agents", "/path/to/agents")
            .add_title("ENVIRONMENT")
            .add_key_value("Version", "1.0.0")
            .add_key_value("Shell", "bash");
        let actual = fixture.to_rows();
        let expected = InfoRows::TwoColumn(vec![
            ("Logs".to_string(), "/path/to/logs".to_string()),
            ("Agents".to_string(), "/path/to/agents".to_string()),
            ("Version".to_string(), "1.0.0".to_string()),
            ("Shell".to_string(), "bash".to_string()),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_rows_with_special_characters() {
        let fixture = Info::new()
            .add_key_value("Key with spaces", "Value with spaces")
            .add_key_value("Key-with-dashes", "Value-with-dashes")
            .add_key_value("Key_with_underscores", "Value_with_underscores");
        let actual = fixture.to_rows();
        let expected = InfoRows::TwoColumn(vec![
            (
                "Key with spaces".to_string(),
                "Value with spaces".to_string(),
            ),
            (
                "Key-with-dashes".to_string(),
                "Value-with-dashes".to_string(),
            ),
            (
                "Key_with_underscores".to_string(),
                "Value_with_underscores".to_string(),
            ),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_rows_empty_strings() {
        let fixture = Info::new().add_key_value("", "").add_key("EmptyKey");
        let actual = fixture.to_rows();
        let expected = InfoRows::TwoColumn(vec![
            ("".to_string(), "".to_string()),
            ("EmptyKey".to_string(), "".to_string()),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_info_to_rows_integration_with_format_columns() {
        use crate::cli_format::format_columns;

        // Create an Info struct with typical content
        let info = Info::new()
            .add_title("PATHS")
            .add_key_value("Logs", "/path/to/logs")
            .add_key_value("Agents", "/path/to/agents")
            .add_title("ENVIRONMENT")
            .add_key_value("Version", "1.0.0")
            .add_key_value("Shell", "bash");

        // Convert to rows and verify the structure
        let rows = info.to_rows();
        assert_eq!(
            rows,
            InfoRows::TwoColumn(vec![
                ("Logs".to_string(), "/path/to/logs".to_string()),
                ("Agents".to_string(), "/path/to/agents".to_string()),
                ("Version".to_string(), "1.0.0".to_string()),
                ("Shell".to_string(), "bash".to_string()),
            ])
        );

        // Test that it works with format_columns (should not panic)
        let info1 = Info::new()
            .add_key_value("Setting1", "Value1")
            .add_key_value("Setting2", "Value2");
        let info2 = Info::new()
            .add_key_value("Config1", "Option1")
            .add_key_value("Config2", "Option2");

        // This should work without panicking
        match info1.to_rows() {
            InfoRows::TwoColumn(rows) => format_columns(rows),
            InfoRows::ThreeColumn(rows) => format_columns(rows),
        }
        match info2.to_rows() {
            InfoRows::TwoColumn(rows) => format_columns(rows),
            InfoRows::ThreeColumn(rows) => format_columns(rows),
        }
    }

    #[test]
    fn test_empty_info_with_format_columns() {
        use crate::cli_format::format_columns;
        let empty_info = Info::new();
        // Should not panic with empty info structs
        match empty_info.to_rows() {
            InfoRows::TwoColumn(rows) => format_columns(rows),
            InfoRows::ThreeColumn(rows) => format_columns(rows),
        }
    }

    #[test]
    fn test_to_rows_with_subtitle() {
        let fixture = Info::new()
            .add_key_value_subtitle("Key1", "Value1", "Subtitle1")
            .add_key_value_subtitle("Key2", "Value2", "Subtitle2");
        let actual = fixture.to_rows();
        let expected = InfoRows::ThreeColumn(vec![
            (
                "Key1".to_string(),
                "Value1".to_string(),
                "Subtitle1".to_string(),
            ),
            (
                "Key2".to_string(),
                "Value2".to_string(),
                "Subtitle2".to_string(),
            ),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mixed_info_with_format_columns() {
        use crate::cli_format::format_columns;
        let info1 = Info::new()
            .add_title("TITLE")
            .add_key_value("Key1", "Value1")
            .add_key_value("Key2", "Value2");
        let info2 = Info::new()
            .add_key_value("Key3", "Value3")
            .add_key_value("Key4", "Value4");

        // Should not panic and handle mixed content correctly
        match info1.to_rows() {
            InfoRows::TwoColumn(rows) => format_columns(rows),
            InfoRows::ThreeColumn(rows) => format_columns(rows),
        }
        match info2.to_rows() {
            InfoRows::TwoColumn(rows) => format_columns(rows),
            InfoRows::ThreeColumn(rows) => format_columns(rows),
        }
    }
}
