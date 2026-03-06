//! ZSH right prompt implementation.
//!
//! Provides the right prompt (RPROMPT) display for the ZSH shell integration,
//! showing agent name, model, and token count information.

use std::fmt::{self, Display};

use convert_case::{Case, Casing};
use derive_setters::Setters;
use forge_domain::{AgentId, ModelId, TokenCount};

use super::style::{ZshColor, ZshStyle};
use crate::utils::humanize_number;

/// ZSH right prompt displaying agent, model, and token count.
///
/// Formats shell prompt information with appropriate colors:
/// - Inactive state (no tokens): dimmed colors
/// - Active state (has tokens): bright white/cyan colors
#[derive(Setters)]
pub struct ZshRPrompt {
    agent: Option<AgentId>,
    model: Option<ModelId>,
    token_count: Option<TokenCount>,
    cost: Option<f64>,
    /// Controls whether to render nerd font symbols. Defaults to `true`.
    #[setters(into)]
    use_nerd_font: bool,
    /// Currency symbol for cost display (e.g., "INR", "EUR", "$", "€").
    /// Defaults to "$".
    #[setters(into)]
    currency_symbol: String,
    /// Conversion ratio for cost display. Cost is multiplied by this value.
    /// Defaults to 1.0.
    conversion_ratio: f64,
    /// Force light mode colors. If None, auto-detects from terminal.
    #[setters(strip_option)]
    light_mode: Option<bool>,
}
impl Default for ZshRPrompt {
    fn default() -> Self {
        Self {
            agent: None,
            model: None,
            token_count: None,
            cost: None,
            use_nerd_font: true,
            currency_symbol: "\u{f155}".to_string(),
            conversion_ratio: 1.0,
            light_mode: None,
        }
    }
}

const AGENT_SYMBOL: &str = "\u{f167a}";
const MODEL_SYMBOL: &str = "\u{ec19}";

/// Detects if the terminal is in light mode.
fn is_light_theme() -> bool {
    use terminal_colorsaurus::{QueryOptions, ThemeMode as ColorsaurusThemeMode, theme_mode};

    matches!(
        theme_mode(QueryOptions::default()),
        Ok(ColorsaurusThemeMode::Light)
    )
}

impl Display for ZshRPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let active = *self.token_count.unwrap_or_default() > 0usize;
        let light_mode = self.light_mode.unwrap_or_else(is_light_theme);

        // Add agent
        let agent_id = self.agent.clone().unwrap_or_default();
        let agent_id = if self.use_nerd_font {
            format!(
                "{AGENT_SYMBOL} {}",
                agent_id.to_string().to_case(Case::UpperSnake)
            )
        } else {
            agent_id.to_string().to_case(Case::UpperSnake)
        };
        let styled = if active {
            if light_mode {
                agent_id.zsh().bold().fg(ZshColor::BLACK)
            } else {
                agent_id.zsh().bold().fg(ZshColor::WHITE)
            }
        } else {
            agent_id.zsh().bold().fg(ZshColor::DIMMED)
        };
        write!(f, " {}", styled)?;

        // Add token count
        if let Some(count) = self.token_count {
            let num = humanize_number(*count);

            let prefix = match count {
                TokenCount::Actual(_) => "",
                TokenCount::Approx(_) => "~",
            };

            if active {
                if light_mode {
                    write!(f, " {}{}", prefix, num.zsh().fg(ZshColor::BLACK).bold())?;
                } else {
                    write!(f, " {}{}", prefix, num.zsh().fg(ZshColor::WHITE).bold())?;
                }
            }
        }

        // Add cost
        if let Some(cost) = self.cost
            && active
        {
            let converted_cost = cost * self.conversion_ratio;
            let cost_str = format!("{}{:.2}", self.currency_symbol, converted_cost);
            write!(f, " {}", cost_str.zsh().fg(ZshColor::GREEN).bold())?;
        }

        // Add model
        if let Some(ref model_id) = self.model {
            let model_id = if self.use_nerd_font {
                format!("{MODEL_SYMBOL} {}", model_id)
            } else {
                model_id.to_string()
            };
            let styled = if active {
                // Use darker blue for light mode, cyan for dark mode
                if light_mode {
                    model_id.zsh().fg(ZshColor::BLACK)
                } else {
                    model_id.zsh().fg(ZshColor::CYAN)
                }
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
    fn test_rprompt_init_state() {
        // No tokens = init/dimmed state
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .to_string();

        let expected = " %B%F{240}\u{f167a} FORGE%f%b %F{240}\u{ec19} gpt-4%f";
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

        // Accept either WHITE (dark mode) or BLACK (light mode) for active state
        let expected_white =
            " %B%F{15}\u{f167a} FORGE%f%b %B%F{15}1.5k%f%b %F{134}\u{ec19} gpt-4%f";
        let expected_black = " %B%F{0}\u{f167a} FORGE%f%b %B%F{0}1.5k%f%b %F{0}\u{ec19} gpt-4%f";
        assert!(actual == expected_white || actual == expected_black);
    }

    #[test]
    fn test_rprompt_with_tokens_and_cost() {
        // Tokens > 0 with cost = active/bright state with cost display
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .token_count(Some(TokenCount::Actual(1500)))
            .cost(Some(0.0123))
            .currency_symbol("\u{f155}")
            .to_string();

        // Accept either WHITE (dark mode) or BLACK (light mode) for active state
        let expected_white = " %B%F{15}\u{f167a} FORGE%f%b %B%F{15}1.5k%f%b %B%F{2}\u{f155}0.01%f%b %F{134}\u{ec19} gpt-4%f";
        let expected_black = " %B%F{0}\u{f167a} FORGE%f%b %B%F{0}1.5k%f%b %B%F{2}\u{f155}0.01%f%b %F{0}\u{ec19} gpt-4%f";
        assert!(actual == expected_white || actual == expected_black);
    }

    #[test]
    fn test_rprompt_without_nerdfonts() {
        // Test with nerdfonts disabled
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .token_count(Some(TokenCount::Actual(1500)))
            .use_nerd_font(false)
            .to_string();

        // Accept either WHITE (dark mode) or BLACK (light mode) for active state
        let expected_white = " %B%F{15}FORGE%f%b %B%F{15}1.5k%f%b %F{134}gpt-4%f";
        let expected_black = " %B%F{0}FORGE%f%b %B%F{0}1.5k%f%b %F{0}gpt-4%f";
        assert!(actual == expected_white || actual == expected_black);
    }

    #[test]
    fn test_rprompt_with_currency_conversion() {
        // Test with custom currency symbol and conversion ratio
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .token_count(Some(TokenCount::Actual(1500)))
            .cost(Some(0.01))
            .currency_symbol("INR")
            .conversion_ratio(83.5)
            .to_string();

        // Accept either WHITE (dark mode) or BLACK (light mode) for active state
        let expected_white = " %B%F{15}\u{f167a} FORGE%f%b %B%F{15}1.5k%f%b %B%F{2}INR0.83%f%b %F{134}\u{ec19} gpt-4%f";
        let expected_black =
            " %B%F{0}\u{f167a} FORGE%f%b %B%F{0}1.5k%f%b %B%F{2}INR0.83%f%b %F{0}\u{ec19} gpt-4%f";
        assert!(actual == expected_white || actual == expected_black);
    }
    #[test]
    fn test_rprompt_with_eur_currency() {
        // Test with EUR currency
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .token_count(Some(TokenCount::Actual(1500)))
            .cost(Some(0.01))
            .currency_symbol("€")
            .conversion_ratio(0.92)
            .to_string();

        // Accept either WHITE (dark mode) or BLACK (light mode) for active state
        let expected_white = " %B%F{15}\u{f167a} FORGE%f%b %B%F{15}1.5k%f%b %B%F{2}€0.01%f%b %F{134}\u{ec19} gpt-4%f";
        let expected_black =
            " %B%F{0}\u{f167a} FORGE%f%b %B%F{0}1.5k%f%b %B%F{2}€0.01%f%b %F{0}\u{ec19} gpt-4%f";
        assert!(actual == expected_white || actual == expected_black);
    }
}
