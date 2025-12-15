//! ZSH prompt styling utilities.
//!
//! This module provides helpers for generating ZSH-native prompt escape sequences.
//! Unlike ANSI escape codes, ZSH prompt escapes are interpreted by ZSH's prompt
//! renderer, making them work correctly in PROMPT and RPROMPT contexts.

use std::fmt::{self, Display};

use convert_case::{Case, Casing};
use derive_setters::Setters;
use forge_domain::{AgentId, ModelId, TokenCount};

/// ZSH prompt color using 256-color palette.
///
/// Maps to ZSH's `%F{N}` prompt escape sequence where N is a color code.
#[derive(Debug, Clone, Copy)]
pub struct ZshColor(u8);

impl ZshColor {
    /// White (color 15)
    pub const WHITE: Self = Self(15);
    /// Cyan (color 134)
    pub const CYAN: Self = Self(134);
    /// Dimmed gray (color 240)
    pub const DIMMED: Self = Self(240);

    /// Creates a color from a 256-color palette value.
    #[allow(dead_code)]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
}

impl Display for ZshColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A styled string for ZSH prompts.
///
/// Wraps text with ZSH prompt escape sequences for colors and formatting.
#[derive(Debug, Clone)]
pub struct ZshStyled<'a> {
    text: &'a str,
    fg: Option<ZshColor>,
    bold: bool,
}

impl<'a> ZshStyled<'a> {
    /// Creates a new styled string with the given text.
    pub fn new(text: &'a str) -> Self {
        Self { text, fg: None, bold: false }
    }

    /// Sets the foreground color.
    pub fn fg(mut self, color: ZshColor) -> Self {
        self.fg = Some(color);
        self
    }

    /// Makes the text bold.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

impl Display for ZshStyled<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Opening escapes
        if self.bold {
            write!(f, "%B")?;
        }
        if let Some(ref color) = self.fg {
            write!(f, "%F{{{}}}", color)?;
        }

        // Text content
        write!(f, "{}", self.text)?;

        // Closing escapes (in reverse order)
        if self.fg.is_some() {
            write!(f, "%f")?;
        }
        if self.bold {
            write!(f, "%b")?;
        }

        Ok(())
    }
}

/// Extension trait for styling strings for ZSH prompts.
pub trait ZshStyle {
    /// Creates a ZSH-styled wrapper for this string.
    fn zsh(&self) -> ZshStyled<'_>;
}

impl ZshStyle for str {
    fn zsh(&self) -> ZshStyled<'_> {
        ZshStyled::new(self)
    }
}

impl ZshStyle for String {
    fn zsh(&self) -> ZshStyled<'_> {
        ZshStyled::new(self.as_str())
    }
}

/// ZSH right prompt displaying agent, model, and token count.
///
/// Formats shell prompt information with appropriate colors:
/// - Inactive state (no tokens): dimmed colors
/// - Active state (has tokens): bright white/cyan colors
#[derive(Default, Setters)]
pub struct ZshRPrompt {
    agent: Option<AgentId>,
    model: Option<ModelId>,
    token_count: Option<TokenCount>,
}

impl Display for ZshRPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let active = *self.token_count.unwrap_or_default() > 0usize;

        // Add agent
        if let Some(ref agent_id) = self.agent {
            let agent_id = format!("󱙺 {}", agent_id.to_string().to_case(Case::UpperSnake));
            let styled = if active {
                agent_id.zsh().bold().fg(ZshColor::WHITE)
            } else {
                agent_id.zsh().bold().fg(ZshColor::DIMMED)
            };
            write!(f, " {}", styled)?;
        }

        // Add token count
        if let Some(count) = self.token_count {
            let count = match *count {
                n if n >= 1_000_000_000 => format!("{:.1}B", n as f64 / 1_000_000_000.0),
                n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
                n if n >= 1_000 => format!("{:.1}k", n as f64 / 1_000.0),
                _ => count.to_string(),
            };
            if active {
                write!(f, " {}", count.zsh().fg(ZshColor::WHITE).bold())?;
            }
        }

        // Add model
        if let Some(ref model_id) = self.model {
            let model_id = format!(" {}", model_id.to_string());
            let styled = if active {
                model_id.zsh().fg(ZshColor::CYAN)
            } else {
                model_id.zsh().fg(ZshColor::DIMMED)
            };
            write!(f, " {}", styled)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let actual = "hello".zsh().to_string();
        let expected = "hello";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bold() {
        let actual = "hello".zsh().bold().to_string();
        let expected = "%Bhello%b";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bold_and_color() {
        let actual = "hello".zsh().bold().fg(ZshColor::WHITE).to_string();
        let expected = "%B%F{15}hello%f%b";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fixed_color() {
        let actual = "hello".zsh().fg(ZshColor::new(240)).to_string();
        let expected = "%F{240}hello%f";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_rprompt_init_state() {
        // No tokens = init/dimmed state
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .to_string();
        let expected = " %B%F{240}󱙺 FORGE%f%b %F{240} gpt-4%f";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_rprompt_with_tokens() {
        // Tokens > 0 = active/bright state
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .token_count(Some(TokenCount::Actual(1500)))
            .to_string();
        let expected = " %B%F{15}󱙺 FORGE%f%b %B%F{15}1.5k%f%b %F{14} gpt-4%f";
        assert_eq!(actual, expected);
    }
}
