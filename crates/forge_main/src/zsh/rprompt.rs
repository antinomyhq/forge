//! ZSH right prompt implementation.
//!
//! Provides the right prompt (RPROMPT) display for the ZSH shell integration,
//! showing agent name, model, and token count information.

use std::fmt::{self, Display};

use convert_case::{Case, Casing};
use derive_setters::Setters;
use forge_domain::{AgentId, ModelId, TokenCount};

use super::style::{ZshColor, ZshStyle};
use crate::shell::prompt::ShellPromptData;
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
}
impl Default for ZshRPrompt {
    fn default() -> Self {
        let width = nerd_glyph_width();
        Self {
            agent: None,
            model: None,
            token_count: None,
            cost: None,
            use_nerd_font: true,
            currency_symbol: wrap_glyph(CURRENCY_GLYPH, width),
            conversion_ratio: 1.0,
        }
    }
}

impl ZshRPrompt {
    /// Creates a `ZshRPrompt` from shared [`ShellPromptData`].
    pub fn from_prompt_data(data: &ShellPromptData) -> Self {
        let width = nerd_glyph_width();

        // If the currency symbol is the default "$", use the nerd font glyph
        let currency_symbol = if data.currency_symbol == "$" {
            wrap_glyph(CURRENCY_GLYPH, width)
        } else {
            data.currency_symbol.clone()
        };

        Self {
            agent: data.agent.clone(),
            model: data.model.clone(),
            token_count: data.token_count,
            cost: data.cost,
            use_nerd_font: data.use_nerd_font,
            currency_symbol,
            conversion_ratio: data.conversion_ratio,
        }
    }
}

const AGENT_GLYPH: char = '\u{f167a}';
const MODEL_GLYPH: char = '\u{ec19}';
const CURRENCY_GLYPH: char = '\u{f155}';

// Nerd Font glyphs wrapped with %{…%WG%} so zsh counts each as the correct
// number of visible columns. These Private Use Area codepoints have
// terminal-dependent rendering width (1 or 2 columns). Use the
// NERD_FONT_GLYPH_WIDTH=1 or =2 environment variable to override (default: 2
// for Windows 11).
fn nerd_glyph_width() -> u8 {
    std::env::var("NERD_FONT_GLYPH_WIDTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&w| w == 1 || w == 2)
        .unwrap_or(2)
}

fn wrap_glyph(c: char, width: u8) -> String {
    format!("%{{{}%{}G%}}", c, width)
}

impl Display for ZshRPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let active = *self.token_count.unwrap_or_default() > 0usize;

        // Add agent
        let agent_id = self.agent.clone().unwrap_or_default();
        let agent_id = if self.use_nerd_font {
            let width = nerd_glyph_width();
            format!(
                "{} {}",
                wrap_glyph(AGENT_GLYPH, width),
                agent_id.to_string().to_case(Case::UpperSnake)
            )
        } else {
            agent_id.to_string().to_case(Case::UpperSnake)
        };
        let styled = if active {
            agent_id.zsh().bold().fg(ZshColor::WHITE)
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
                write!(f, " {}{}", prefix, num.zsh().fg(ZshColor::WHITE).bold())?;
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

        if let Some(ref model_id) = self.model {
            let model_id = if self.use_nerd_font {
                let width = nerd_glyph_width();
                format!("{} {}", wrap_glyph(MODEL_GLYPH, width), model_id)
            } else {
                model_id.to_string()
            };
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
    fn test_rprompt_init_state() {
        // No tokens = init/dimmed state
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .to_string();

        let expected = " %B%F{240}%{\u{f167a}%2G%} FORGE%f%b %F{240}%{\u{ec19}%2G%} gpt-4%f";
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

        let expected =
            " %B%F{15}%{\u{f167a}%2G%} FORGE%f%b %B%F{15}1.5k%f%b %F{134}%{\u{ec19}%2G%} gpt-4%f";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_rprompt_with_tokens_and_cost() {
        // Tokens > 0 with cost = active/bright state with cost display
        let actual = ZshRPrompt::default()
            .agent(Some(AgentId::new("forge")))
            .model(Some(ModelId::new("gpt-4")))
            .token_count(Some(TokenCount::Actual(1500)))
            .cost(Some(0.0123))
            .currency_symbol("%{\u{f155}%2G%}")
            .to_string();

        let expected = " %B%F{15}%{\u{f167a}%2G%} FORGE%f%b %B%F{15}1.5k%f%b %B%F{2}%{\u{f155}%2G%}0.01%f%b %F{134}%{\u{ec19}%2G%} gpt-4%f";
        assert_eq!(actual, expected);
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

        let expected = " %B%F{15}FORGE%f%b %B%F{15}1.5k%f%b %F{134}gpt-4%f";
        assert_eq!(actual, expected);
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

        let expected = " %B%F{15}%{\u{f167a}%2G%} FORGE%f%b %B%F{15}1.5k%f%b %B%F{2}INR0.83%f%b %F{134}%{\u{ec19}%2G%} gpt-4%f";
        assert_eq!(actual, expected);
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

        let expected = " %B%F{15}%{\u{f167a}%2G%} FORGE%f%b %B%F{15}1.5k%f%b %B%F{2}€0.01%f%b %F{134}%{\u{ec19}%2G%} gpt-4%f";
        assert_eq!(actual, expected);
    }
}
