use std::borrow::Cow;
use std::env;
use std::fmt::Write;
use std::process::Command;

use derive_setters::Setters;
use forge_api::{ModelId, Usage};
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
    pub usage: Option<Usage>,
    pub mode: Mode,
    pub model: Option<ModelId>,
}

impl Prompt for ForgePrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        // Pre-compute styles to avoid repeated style creation
        let mode_style = Style::new().fg(Color::White).bold();
        let folder_style = Style::new().fg(Color::Cyan);
        let branch_style = Style::new().fg(Color::LightGreen);

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
        write!(
            result,
            "{} {}",
            mode_style.paint(self.mode.to_string()),
            folder_style.paint(&current_dir)
        )
        .unwrap();

        // Only append branch info if present
        if let Some(branch) = branch_opt {
            if branch != current_dir {
                write!(result, " {} ", branch_style.paint(branch)).unwrap();
            }
        }

        write!(result, "\n{} ", branch_style.paint(RIGHT_CHEVRON)).unwrap();

        Cow::Owned(result)
    }

    fn render_prompt_right(&self) -> Cow<str> {
        // Use a string buffer with pre-allocation to reduce allocations
        let mut result = String::with_capacity(32);

        // Start with bracket and version
        write!(result, "[{VERSION}").unwrap();

        // Append model if available
        if let Some(model) = self.model.as_ref() {
            let model_str = model.to_string();
            let formatted_model = model_str
                .split('/')
                .next_back()
                .unwrap_or_else(|| model.as_str());
            write!(result, "/{formatted_model}").unwrap();
        }

        // Append usage info
        let reported = self
            .usage
            .as_ref()
            .unwrap_or(&Usage::default())
            .total_tokens;

        let estimated = self
            .usage
            .as_ref()
            .and_then(|u| u.estimated_tokens)
            .unwrap_or(0);

        if estimated > reported {
            write!(result, "/~{estimated}").unwrap();
        } else {
            write!(result, "/{reported}").unwrap();
        }

        write!(result, "]").unwrap();

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
            write!(result, "({prefix}reverse-search) ").unwrap();
        } else {
            write!(
                result,
                "({}reverse-search: {}) ",
                prefix, history_search.term
            )
            .unwrap();
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
    fn test_render_prompt_right_with_usage() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            estimated_tokens: None,
        };
        let mut prompt = ForgePrompt::default();
        prompt.usage(usage);

        let actual = prompt.render_prompt_right();
        assert!(actual.contains(&VERSION.to_string()));
        assert!(actual.contains("30"));
    }

    #[test]
    fn test_render_prompt_right_without_usage() {
        let prompt = ForgePrompt::default();
        let actual = prompt.render_prompt_right();
        assert!(actual.contains(&VERSION.to_string()));
        assert!(actual.contains("0"));
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

    #[test]
    fn test_render_prompt_right_with_model() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            estimated_tokens: None,
        };
        let mut prompt = ForgePrompt::default();
        prompt.usage(usage);
        prompt.model(ModelId::new("anthropic/claude-3"));

        let actual = prompt.render_prompt_right();
        assert!(actual.contains("claude-3")); // Only the last part after splitting by '/'
        assert!(!actual.contains("anthropic/claude-3")); // Should not contain the full model ID
        assert!(actual.contains(&VERSION.to_string()));
        assert!(actual.contains("30"));
    }
}
