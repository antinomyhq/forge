use std::borrow::Cow;

use derive_setters::Setters;
use forge_api::Usage;
use nu_ansi_term::{Color, Style};
use reedline::{Prompt, PromptHistorySearchStatus};

// Constants
pub const MAX_LEN: usize = 30;
const AI_INDICATOR: &str = "⚡";
const MULTILINE_INDICATOR: &str = "::: ";
const RIGHT_CHEVRON: &str = "❯";
const TRUNCATION_INDICATOR: &str = "…";

/// Very Specialized Prompt for the Agent Chat
#[derive(Clone, Default, Setters, Debug)]
#[setters(strip_option, borrow_self)]
pub struct ForgePrompt {
    title: Option<String>,
    usage: Option<Usage>,
}

impl Prompt for ForgePrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        if let Some(title) = self.title.as_ref() {
            let title = if title.len() > MAX_LEN {
                format!(
                    "{}{}",
                    &title[..MAX_LEN - TRUNCATION_INDICATOR.len()],
                    TRUNCATION_INDICATOR
                )
            } else {
                title.clone()
            };
            Cow::Owned(format!(
                "{AI_INDICATOR} {} {} ",
                Style::new().fg(Color::Cyan).paint(title),
                Style::new().fg(Color::LightYellow).paint(RIGHT_CHEVRON),
            ))
        } else {
            Cow::Borrowed(AI_INDICATOR)
        }
    }

    fn render_prompt_right(&self) -> Cow<str> {
        if let Some(usage) = self.usage.as_ref() {
            let usage_text = format!(
                "[{}/{}/{}]",
                usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
            );
            Cow::Owned(
                Style::new()
                    .bold()
                    .fg(Color::DarkGray)
                    .paint(usage_text)
                    .to_string(),
            )
        } else {
            Cow::Borrowed("")
        }
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
        let input = format!("({}reverse-search: {}) ", prefix, history_search.term);
        Cow::Owned(Style::new().fg(Color::White).paint(input).to_string())
    }
}

#[cfg(test)]
mod tests {
    use nu_ansi_term::Style;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_render_prompt_left_with_title() {
        let mut prompt = ForgePrompt::default();
        prompt.title("test-title".to_string());
        let title_style = Style::new().fg(Color::Cyan).paint("test-title").to_string();
        let chevron_style = Style::new()
            .fg(Color::LightYellow)
            .paint(RIGHT_CHEVRON)
            .to_string();
        let actual = prompt.render_prompt_left();
        let expected = format!("{AI_INDICATOR} {title_style} {chevron_style} ");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_left_without_title() {
        let prompt = ForgePrompt::default();
        let actual = prompt.render_prompt_left();
        let expected = AI_INDICATOR;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_left_with_long_title() {
        let long_title = "a".repeat(MAX_LEN + 10);
        let mut prompt = ForgePrompt::default();
        prompt.title(long_title);
        let truncated_title = format!(
            "{}{}",
            "a".repeat(MAX_LEN - TRUNCATION_INDICATOR.len()),
            TRUNCATION_INDICATOR
        );
        let title_style = Style::new()
            .fg(Color::Cyan)
            .paint(truncated_title)
            .to_string();
        let actual = prompt.render_prompt_left();
        let chevron_style = Style::new()
            .fg(Color::LightYellow)
            .paint(RIGHT_CHEVRON)
            .to_string();
        let expected = format!("{AI_INDICATOR} {title_style} {chevron_style} ");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_right_with_usage() {
        let usage = Usage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 };
        let mut prompt = ForgePrompt::default();
        prompt.usage(usage);
        let usage_style = Style::new()
            .bold()
            .fg(Color::DarkGray)
            .paint("[10/20/30]")
            .to_string();
        let actual = prompt.render_prompt_right();
        let expected = usage_style;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_right_without_usage() {
        let prompt = ForgePrompt::default();
        let actual = prompt.render_prompt_right();
        let expected = "";
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
    fn test_render_prompt_left_with_long_title_length() {
        let long_title = "a".repeat(MAX_LEN * 2); // Much longer than MAX_LEN
        let mut prompt = ForgePrompt::default();
        prompt.title(long_title);
        let actual = prompt.render_prompt_left().into_owned();

        // Extract just the title part (remove the AI_INDICATOR and formatting)
        let title_start = actual.find('a').unwrap_or(0);
        let title_end = actual
            .rfind(TRUNCATION_INDICATOR)
            .map(|i| i + TRUNCATION_INDICATOR.len())
            .unwrap_or(actual.len());
        let just_title = &actual[title_start..title_end];

        assert!(
            just_title.len() <= MAX_LEN,
            "Title length {} exceeds MAX_LEN {}: '{}'",
            just_title.len(),
            MAX_LEN,
            just_title
        );
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
}
