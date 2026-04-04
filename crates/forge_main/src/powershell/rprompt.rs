//! PowerShell right prompt implementation.
//!
//! Provides the right prompt display for PowerShell integration,
//! showing agent name, model, token count, and cost using ANSI escape codes.

use std::fmt::{self, Display};

use convert_case::{Case, Casing};
use forge_domain::TokenCount;

use crate::shell::prompt::ShellPromptData;
use crate::shell::style::{AnsiColor, AnsiStyle};
use crate::utils::humanize_number;

/// Nerd Font glyph constants (shared with zsh, but no width wrapping needed
/// since Windows Terminal handles glyph width natively).
const AGENT_GLYPH: char = '\u{f167a}';
const MODEL_GLYPH: char = '\u{ec19}';
const CURRENCY_GLYPH: char = '\u{f155}';

/// PowerShell prompt displaying agent, model, token count, and cost.
///
/// Uses ANSI escape codes for styling:
/// - Inactive state (no tokens): dimmed colors
/// - Active state (has tokens): bright white/cyan colors
pub struct PowerShellRPrompt {
    data: ShellPromptData,
}

impl PowerShellRPrompt {
    /// Creates a `PowerShellRPrompt` from shared [`ShellPromptData`].
    pub fn from_prompt_data(data: ShellPromptData) -> Self {
        Self { data }
    }
}

impl Display for PowerShellRPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let active = self
            .data
            .token_count
            .map(|tc| *tc > 0usize)
            .unwrap_or(false);

        // Agent
        let agent_name = self
            .data
            .agent
            .as_ref()
            .map(|a| a.to_string().to_case(Case::UpperSnake))
            .unwrap_or_else(|| "FORGE".to_string());

        let agent_str = if self.data.use_nerd_font {
            format!("{} {}", AGENT_GLYPH, agent_name)
        } else {
            agent_name
        };

        let (agent_color, model_color) = if active {
            (AnsiColor::WHITE, AnsiColor::CYAN)
        } else {
            (AnsiColor::DIMMED, AnsiColor::DIMMED)
        };

        write!(f, " {}", agent_str.ansi().bold().fg(agent_color))?;

        // Token count
        if let Some(count) = self.data.token_count {
            let num = humanize_number(*count);
            let prefix = match count {
                TokenCount::Actual(_) => "",
                TokenCount::Approx(_) => "~",
            };
            if active {
                write!(
                    f,
                    " {}",
                    format!("{}{}", prefix, num)
                        .ansi()
                        .bold()
                        .fg(AnsiColor::WHITE)
                )?;
            }
        }

        // Cost
        if let Some(cost) = self.data.cost
            && active {
                let converted = cost * self.data.conversion_ratio;
                let currency = if self.data.use_nerd_font && self.data.currency_symbol == "$" {
                    CURRENCY_GLYPH.to_string()
                } else {
                    self.data.currency_symbol.clone()
                };
                let cost_str = format!("{}{:.2}", currency, converted);
                write!(f, " {}", cost_str.ansi().bold().fg(AnsiColor::GREEN))?;
            }

        // Model
        if let Some(ref model_id) = self.data.model {
            let model_str = if self.data.use_nerd_font {
                format!("{} {}", MODEL_GLYPH, model_id)
            } else {
                model_id.to_string()
            };
            write!(f, " {}", model_str.ansi().fg(model_color))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_api::{AgentId, ModelId};
    use forge_domain::TokenCount;

    use super::*;

    fn make_data(
        agent: Option<&str>,
        model: Option<&str>,
        token_count: Option<TokenCount>,
        cost: Option<f64>,
    ) -> ShellPromptData {
        ShellPromptData {
            agent: agent.map(AgentId::new),
            model: model.map(ModelId::new),
            token_count,
            cost,
            use_nerd_font: true,
            currency_symbol: "$".to_string(),
            conversion_ratio: 1.0,
        }
    }

    #[test]
    fn test_init_state_dimmed() {
        let data = make_data(Some("forge"), Some("gpt-4"), None, None);
        let prompt = PowerShellRPrompt::from_prompt_data(data);
        let output = prompt.to_string();

        // Should use dimmed color (90 = dark gray) for both agent and model
        assert!(
            output.contains("\x1b[1;90m"),
            "agent should be dimmed: {}",
            output
        );
        assert!(output.contains("FORGE"));
        assert!(output.contains("gpt-4"));
    }

    #[test]
    fn test_active_state_with_tokens() {
        let data = make_data(
            Some("forge"),
            Some("gpt-4"),
            Some(TokenCount::Actual(1500)),
            None,
        );
        let prompt = PowerShellRPrompt::from_prompt_data(data);
        let output = prompt.to_string();

        // Should use bright white (97) for agent/tokens and cyan (36) for model
        assert!(
            output.contains("\x1b[1;97m"),
            "agent should be bright white: {}",
            output
        );
        assert!(
            output.contains("\x1b[36m"),
            "model should be cyan: {}",
            output
        );
        assert!(output.contains("1.5k"));
    }

    #[test]
    fn test_active_state_with_cost() {
        let data = ShellPromptData {
            agent: Some(AgentId::new("forge")),
            model: Some(ModelId::new("gpt-4")),
            token_count: Some(TokenCount::Actual(1500)),
            cost: Some(0.0123),
            use_nerd_font: false,
            currency_symbol: "$".to_string(),
            conversion_ratio: 1.0,
        };
        let prompt = PowerShellRPrompt::from_prompt_data(data);
        let output = prompt.to_string();

        assert!(output.contains("$0.01"));
        assert!(
            output.contains("\x1b[1;32m"),
            "cost should be green: {}",
            output
        );
    }

    #[test]
    fn test_without_nerd_font() {
        let data = ShellPromptData {
            agent: Some(AgentId::new("forge")),
            model: Some(ModelId::new("gpt-4")),
            token_count: Some(TokenCount::Actual(1500)),
            cost: None,
            use_nerd_font: false,
            currency_symbol: "$".to_string(),
            conversion_ratio: 1.0,
        };
        let prompt = PowerShellRPrompt::from_prompt_data(data);
        let output = prompt.to_string();

        // Should NOT contain nerd font glyphs
        assert!(!output.contains(AGENT_GLYPH));
        assert!(!output.contains(MODEL_GLYPH));
        assert!(output.contains("FORGE"));
        assert!(output.contains("gpt-4"));
    }
}
