use std::borrow::Cow;
use std::env;
use std::fmt::Write;
use std::process::Command;

use derive_setters::Setters;
use forge_api::Usage;
use forge_tracker::VERSION;
use nu_ansi_term::{Color, Style};
use reedline::{Prompt, PromptHistorySearchStatus};

use crate::state::Mode;

// Constants
const MULTILINE_INDICATOR: &str = "::: ";
const RIGHT_CHEVRON: &str = "❯";

/// Very Specialized Prompt for the Agent Chat
#[derive(Clone, Default, Setters)]
#[setters(strip_option, borrow_self)]
pub struct ForgePrompt {
    pub title: Option<String>,
    pub usage: Option<Usage>,
    pub mode: Mode,
}

impl Prompt for ForgePrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        // Pre-compute styles to avoid repeated style creation
        let white_bold = Style::new().fg(Color::White).bold();
        let cyan = Style::new().fg(Color::Cyan);
        let yellow = Style::new().fg(Color::LightYellow);

        // Get current directory
        let current_dir = env::current_dir()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Get git branch (only if we're in a git repo)
        let branch_opt = get_git_branch();

        // Use a string buffer to reduce allocations
        let mut result = String::with_capacity(64); // Pre-allocate a reasonable size

        // Build the string step-by-step
        let _ = write!(
            result,
            "{} {}",
            white_bold.paint(self.mode.to_string()),
            cyan.paint(&current_dir)
        );

        // Only append branch info if present
        if let Some(branch) = branch_opt {
            let _ = write!(result, " {} ", yellow.paint(branch));
        }

        let _ = write!(result, "\n{} ", yellow.paint(RIGHT_CHEVRON));

        Cow::Owned(result)
    }

    fn render_prompt_right(&self) -> Cow<str> {
        let usage = self
            .usage
            .as_ref()
            .unwrap_or(&Usage::default())
            .total_tokens;

        // Use a string buffer with pre-allocation to reduce allocations
        let mut result = String::with_capacity(32);

        // Append title if present
        if let Some(title) = self.title.as_ref() {
            let _ = write!(result, "{} ", title);
        }

        // Append usage info
        let _ = write!(result, "[{}/{}]", VERSION, usage);

        // Apply styling once at the end
        Cow::Owned(
            Style::new()
                .bold()
                .fg(Color::DarkGray)
                .paint(&result)
                .to_string(),
        )
    }

    fn render_prompt_indicator(&self, _prompt_mode: reedline::PromptEditMode) -> Cow<str> {
        Cow::Borrowed("")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        Cow::Borrowed(MULTILINE_INDICATOR)
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: reedline::PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };

        let mut result = String::with_capacity(32);

        // Handle empty search term more elegantly
        if history_search.term.is_empty() {
            let _ = write!(result, "({}reverse-search) ", prefix);
        } else {
            let _ = write!(
                result,
                "({}reverse-search: {}) ",
                prefix, history_search.term
            );
        }

        Cow::Owned(Style::new().fg(Color::White).paint(&result).to_string())
    }
}

/// Gets the current git branch name if available
fn get_git_branch() -> Option<String> {
    // First check if we're in a git repository
    let git_check = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .ok()?;

    if !git_check.status.success() || git_check.stdout.is_empty() {
        return None;
    }

    // If we are in a git repo, get the branch
    let output = Command::new("git")
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

#[cfg(test)]
mod tests {
    use nu_ansi_term::Style;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_render_prompt_left() {
        let prompt = ForgePrompt::default();

        let actual = prompt.render_prompt_left();

        // Check that it has the expected format with mode and directory displayed
        assert!(actual.contains("ACT"));
        assert!(actual.contains(RIGHT_CHEVRON));
    }

    #[test]
    fn test_render_prompt_left_with_custom_prompt() {
        // Set $PROMPT environment variable temporarily for this test
        env::set_var("PROMPT", "CUSTOM_TEST_PROMPT");

        let prompt = ForgePrompt::default();
        let actual = prompt.render_prompt_left();

        // Clean up after test
        env::remove_var("PROMPT");

        // Verify the prompt contains expected elements regardless of $PROMPT var
        assert!(actual.contains("ACT"));
        assert!(actual.contains(RIGHT_CHEVRON));
    }

    #[test]
    fn test_render_prompt_right_with_title_and_usage() {
        let usage = Usage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 };
        let mut prompt = ForgePrompt::default();
        prompt.usage(usage);
        prompt.title("test-title".to_string());

        let usage_style = Style::new()
            .bold()
            .fg(Color::DarkGray)
            .paint(format!("test-title [{}/30]", VERSION))
            .to_string();

        let actual = prompt.render_prompt_right();
        let expected = usage_style;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_right_with_usage_no_title() {
        let usage = Usage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 };
        let mut prompt = ForgePrompt::default();
        prompt.usage(usage);

        let usage_style = Style::new()
            .bold()
            .fg(Color::DarkGray)
            .paint(format!("[{}/30]", VERSION))
            .to_string();

        let actual = prompt.render_prompt_right();
        let expected = usage_style;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_right_without_usage() {
        let prompt = ForgePrompt::default();
        let actual = prompt.render_prompt_right();
        let expected = Style::new()
            .bold()
            .fg(Color::DarkGray)
            .paint(format!("[{}/0]", VERSION))
            .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_indicator_with_title() {
        let mut prompt = ForgePrompt::default();
        prompt.title("test".to_string());

        let actual = prompt.render_prompt_indicator(reedline::PromptEditMode::Default);
        let expected = "";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_indicator_without_title() {
        let prompt = ForgePrompt::default();
        let actual = prompt.render_prompt_indicator(reedline::PromptEditMode::Default);
        let expected = "";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_multiline_indicator() {
        let prompt = ForgePrompt::default();
        let actual = prompt.render_prompt_multiline_indicator();
        let expected = MULTILINE_INDICATOR;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_history_search_indicator_passing() {
        let prompt = ForgePrompt::default();
        let history_search = reedline::PromptHistorySearch {
            status: PromptHistorySearchStatus::Passing,
            term: "test".to_string(),
        };
        let actual = prompt.render_prompt_history_search_indicator(history_search);
        let expected = Style::new()
            .fg(Color::White)
            .paint("(reverse-search: test) ")
            .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_history_search_indicator_failing() {
        let prompt = ForgePrompt::default();
        let history_search = reedline::PromptHistorySearch {
            status: PromptHistorySearchStatus::Failing,
            term: "test".to_string(),
        };
        let actual = prompt.render_prompt_history_search_indicator(history_search);
        let expected = Style::new()
            .fg(Color::White)
            .paint("(failing reverse-search: test) ")
            .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_history_search_indicator_empty_term() {
        let prompt = ForgePrompt::default();
        let history_search = reedline::PromptHistorySearch {
            status: PromptHistorySearchStatus::Passing,
            term: "".to_string(),
        };
        let actual = prompt.render_prompt_history_search_indicator(history_search);
        let expected = Style::new()
            .fg(Color::White)
            .paint("(reverse-search) ")
            .to_string();
        assert_eq!(actual, expected);
    }
}
