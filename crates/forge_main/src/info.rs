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
    Items(Option<String>, String), // key, value, subtitle
}

impl Section {
    pub fn key(&self) -> Option<&str> {
        match self {
            Section::Items(Some(key), _) => Some(key.as_str()),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct Info {
    sections: Vec<Section>,
}

impl Info {
    pub fn new() -> Self {
        Info { sections: Vec::new() }
    }

    /// Returns a reference to the sections
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn add_title(mut self, title: impl ToString) -> Self {
        self.sections.push(Section::Title(title.to_string()));
        self
    }

    pub fn add_value(self, value: impl ToString) -> Self {
        self.add_item(None::<String>, value)
    }

    pub fn add_key_value(self, key: impl ToString, value: impl ToString) -> Self {
        self.add_item(Some(key), value)
    }

    fn add_item(mut self, key: Option<impl ToString>, value: impl ToString) -> Self {
        self.sections.push(Section::Items(
            key.map(|a| a.to_string()),
            value.to_string(),
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

        // Add file changes section
        if metrics.files_changed.is_empty() {
            info = info.add_value("[No Changes Produced]");
        } else {
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

                info = info.add_key_value(format!("⦿ {filename}"), changes);
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
        let mut width: Option<usize> = None;

        for (i, section) in self.sections.iter().enumerate() {
            match section {
                Section::Title(title) => {
                    writeln!(f)?;
                    writeln!(f, "{}", title.bold().dimmed())?;

                    // Calculate max key width for items under this title
                    width = self
                        .sections
                        .iter()
                        .skip(i + 1)
                        .take_while(|s| matches!(s, Section::Items(..)))
                        .filter_map(|s| s.key())
                        .map(|key| key.len())
                        .max();
                }
                Section::Items(key, value) => {
                    if let Some(key) = key {
                        if let Some(width) = width {
                            writeln!(
                                f,
                                "  {} {}",
                                format!("{key:<width$}:").bright_cyan().bold(),
                                value
                            )?;
                        } else {
                            // No section width (items without a title)
                            writeln!(f, "  {}: {}", key.bright_cyan().bold(), value)?;
                        }
                    } else {
                        // Show value-only items
                        writeln!(f, "  {}", value)?;
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

/// Extracts the first line of raw content from a context message.
fn format_user_message(msg: &forge_api::ContextMessage) -> Option<String> {
    let content = msg.raw_content().and_then(|v| v.as_str())?;
    let trimmed = content.lines().next().unwrap_or(content);
    Some(trimmed.to_string())
}

impl From<&Conversation> for Info {
    fn from(conversation: &Conversation) -> Self {
        let mut info = Info::new().add_title("CONVERSATION");

        info = info.add_key_value("ID", conversation.id.to_string());

        if let Some(title) = &conversation.title {
            info = info.add_key_value("Title", title);
        }

        // Add task and feedback (if available)
        let user_sequences = conversation.first_user_messages();

        if let Some(first_msg) = user_sequences.first()
            && let Some(task) = format_user_message(first_msg)
        {
            info = info.add_key_value("Task", task);
        }

        if user_sequences.len() > 1
            && let Some(last_msg) = user_sequences.last()
            && let Some(feedback) = format_user_message(last_msg)
        {
            info = info.add_key_value("Feedback", feedback);
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;
    use forge_api::Environment;
    use pretty_assertions::assert_eq;

    // Helper to create minimal test environment
    fn create_env(os: &str, home: Option<&str>) -> Environment {
        use fake::{Fake, Faker};
        let mut fixture: Environment = Faker.fake();
        fixture = fixture.os(os.to_string());
        if let Some(home_path) = home {
            fixture = fixture.home(PathBuf::from(home_path));
        }
        fixture
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
        let expected = "C:\\Users\\User\\project";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_path_for_display_windows_home_with_spaces() {
        let fixture = create_env("windows", Some("C:\\Users\\User Name"));
        let path = PathBuf::from("C:\\Users\\User Name\\project");

        let actual = super::format_path_for_display(&fixture, &path);
        let expected = "\"C:\\Users\\User Name\\project\"";
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
    fn test_conversation_info_display_with_task() {
        use chrono::Utc;
        use forge_api::{Context, ContextMessage, ConversationId, Role};

        use super::{Conversation, Metrics};

        let conversation_id = ConversationId::generate();
        let metrics = Metrics::new().with_time(Utc::now());

        // Create a context with user messages
        let context = Context::default()
            .add_message(ContextMessage::system("System prompt"))
            .add_message(ContextMessage::Text(forge_domain::TextMessage {
                role: Role::User,
                content: "First user message".to_string(),
                raw_content: Some(serde_json::json!("First user message")),
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }))
            .add_message(ContextMessage::assistant("Assistant response", None, None))
            .add_message(ContextMessage::Text(forge_domain::TextMessage {
                role: Role::User,
                content: "Create a new feature".to_string(),
                raw_content: Some(serde_json::json!("Create a new feature")),
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }));

        let fixture = Conversation {
            id: conversation_id,
            title: Some("Test Task".to_string()),
            context: Some(context),
            metrics,
            metadata: forge_domain::MetaData::new(Utc::now()),
        };

        let actual = super::Info::from(&fixture);
        let expected_display = actual.to_string();

        // Verify it contains the conversation section with task
        assert!(expected_display.contains("CONVERSATION"));
        assert!(expected_display.contains("Test Task"));
        // Check for Task separately due to ANSI color codes
        assert!(expected_display.contains("Task"));
        assert!(expected_display.contains("Create a new feature"));
        assert!(expected_display.contains(&conversation_id.to_string()));
    }

    #[test]
    fn test_info_display_with_consistent_key_padding() {
        use super::Info;

        let fixture = Info::new()
            .add_title("SECTION ONE")
            .add_key_value("Short", "value1")
            .add_key_value("Very Long Key", "value2")
            .add_key_value("Mid", "value3")
            .add_title("SECTION TWO")
            .add_key_value("A", "valueA")
            .add_key_value("ABC", "valueB");

        let actual = fixture.to_string();

        // Strip ANSI codes for easier assertion
        let stripped = strip_ansi_escapes::strip(&actual);
        let actual_str = String::from_utf8(stripped).unwrap();

        // Verify that keys are padded within each section
        // In SECTION ONE, all keys should be padded to length of "Very Long Key" (13)
        // In SECTION TWO, all keys should be padded to length of "ABC" (3)

        // Check that the display contains properly formatted sections
        assert!(actual_str.contains("SECTION ONE"));
        assert!(actual_str.contains("SECTION TWO"));

        // Verify padding by checking alignment of colons
        // All keys in a section should have colons at the same column
        let lines: Vec<&str> = actual_str.lines().collect();

        // Find SECTION ONE items
        let section_one_start = lines
            .iter()
            .position(|l| l.contains("SECTION ONE"))
            .unwrap();
        let section_two_start = lines
            .iter()
            .position(|l| l.contains("SECTION TWO"))
            .unwrap();

        let section_one_items: Vec<&str> = lines[section_one_start + 1..section_two_start]
            .iter()
            .filter(|l| l.contains(':'))
            .copied()
            .collect();

        // All colons in section one should be at the same position
        let colon_positions: Vec<usize> = section_one_items
            .iter()
            .map(|line| line.find(':').unwrap())
            .collect();

        assert!(
            colon_positions.windows(2).all(|w| w[0] == w[1]),
            "Keys in SECTION ONE should have consistent padding. Colon positions: {:?}",
            colon_positions
        );

        // Check SECTION TWO items
        let section_two_items: Vec<&str> = lines[section_two_start + 1..]
            .iter()
            .filter(|l| l.contains(':'))
            .copied()
            .collect();

        let colon_positions_two: Vec<usize> = section_two_items
            .iter()
            .map(|line| line.find(':').unwrap())
            .collect();

        assert!(
            colon_positions_two.windows(2).all(|w| w[0] == w[1]),
            "Keys in SECTION TWO should have consistent padding. Colon positions: {:?}",
            colon_positions_two
        );

        // Verify that different sections can have different padding
        // (SECTION ONE should have wider padding than SECTION TWO)
        assert!(
            colon_positions[0] > colon_positions_two[0],
            "SECTION ONE should have wider padding than SECTION TWO"
        );
    }
}
