//! ZSH prompt styling utilities.
//!
//! This module provides helpers for generating ZSH-native prompt escape sequences.
//! Unlike ANSI escape codes, ZSH prompt escapes are interpreted by ZSH's prompt
//! renderer, making them work correctly in PROMPT and RPROMPT contexts.

use std::fmt::{self, Display};

use convert_case::{Case, Casing};
use derive_setters::Setters;
use forge_domain::{AgentId, ModelId, TokenCount};

/// ZSH prompt colors.
///
/// These map to ZSH's `%F{color}` prompt escape sequences.
#[derive(Debug, Clone, Copy)]
pub enum ZshColor {
    /// Standard white color
    White,
    /// Standard cyan color
    Cyan,
    /// 256-color palette (0-255)
    Fixed(u8),
}

impl Display for ZshColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZshColor::White => write!(f, "white"),
            ZshColor::Cyan => write!(f, "cyan"),
            ZshColor::Fixed(n) => write!(f, "{}", n),
        }
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
/// - Initial state (no tokens): bright white/cyan colors
/// - Active state (has tokens): dimmed colors
#[derive(Default, Setters)]
pub struct ZshRPrompt {
    agent: Option<AgentId>,
    model: Option<ModelId>,
    token_count: Option<TokenCount>,
}

impl Display for ZshRPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let init = *self.token_count.unwrap_or_default() > 0usize;
        let dimmed = ZshColor::Fixed(240);

        // Add agent
        if let Some(ref agent_id) = self.agent {
            let agent_id = format!("󱙺 {}", agent_id.to_string().to_case(Case::UpperSnake));
            let styled = if init {
                agent_id.zsh().bold().fg(ZshColor::White)
            } else {
                agent_id.zsh().bold().fg(dimmed)
            };
            write!(f, "{} ", styled)?;
        }

        // Add token count
        if let Some(count) = self.token_count {
            let count = match *count {
                n if n >= 1_000_000_000 => format!("{:.1}B", n as f64 / 1_000_000_000.0),
                n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
                n if n >= 1_000 => format!("{:.1}K", n as f64 / 1_000.0),
                _ => count.to_string(),
            };
            if init {
                write!(f, "{}", count.zsh().bold().fg(ZshColor::White))?;
            }
        }

        // Add model
        if let Some(ref model_id) = self.model {
            let model_id = format!(" {}", model_id.to_string());
            let styled = if init {
                model_id.zsh().fg(ZshColor::Cyan)
            } else {
                model_id.zsh().fg(dimmed)
            };
            write!(f, "{}", styled)?;
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
    fn test_color() {
        let actual = "hello".zsh().fg(ZshColor::Cyan).to_string();
        let expected = "%F{cyan}hello%f";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bold_and_color() {
        let actual = "hello".zsh().bold().fg(ZshColor::White).to_string();
        let expected = "%B%F{white}hello%f%b";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fixed_color() {
        let actual = "hello".zsh().fg(ZshColor::Fixed(240)).to_string();
        let expected = "%F{240}hello%f";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_rprompt_init_state() {
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .to_string();
        let expected = "%B%F{white}󱙺 FORGE%f%b %F{cyan} gpt-4%f";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_rprompt_with_tokens() {
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .token_count(Some(TokenCount::Actual(1500)))
            .to_string();
        // When tokens > 0, colors are dimmed (240)
        let expected = "%B%F{240}󱙺 FORGE%f%b %F{240} gpt-4%f";
        assert_eq!(actual, expected);
    }
}
