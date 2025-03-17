use std::borrow::Cow;

use derive_setters::Setters;
use forge_api::Usage;
use forge_tracker::VERSION;
use nu_ansi_term::{Color, Style};
use reedline::{Prompt, PromptHistorySearchStatus};

use crate::state::Mode;

// Constants
const AI_INDICATOR: &str = "⚡";
const MULTILINE_INDICATOR: &str = "::: ";
const RIGHT_CHEVRON: &str = "❯";

/// Very Specialized Prompt for the Agent Chat
#[derive(Clone, Default, Setters)]
#[setters(strip_option, borrow_self)]
pub struct ForgePrompt {
    title: Option<String>,
    usage: Option<Usage>,
    mode: Mode,
}

impl Prompt for ForgePrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        if let Some(title) = self.title.as_ref() {
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
        let usage = self
            .usage
            .as_ref()
            .unwrap_or(&Usage::default())
            .total_tokens;
        let usage_text = format!("[{}/{}/{}]", self.mode, VERSION, usage);
        Cow::Owned(
            Style::new()
                .bold()
                .fg(Color::DarkGray)
                .paint(usage_text)
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
    fn test_render_prompt_right_with_usage() {
        let usage = Usage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 };
        let mut prompt = ForgePrompt::default();
        prompt.usage(usage);
        let usage_style = Style::new()
            .bold()
            .fg(Color::DarkGray)
            .paint(format!("[ACT/{}/30]", VERSION))
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
            .paint(format!("[ACT/{}/0]", VERSION))
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
}
