//! Shared UI display components for model, provider, and agent selection
//!
//! This module provides reusable display wrappers that format domain types
//! for interactive selection menus, eliminating code duplication within
//! forge_main.

use std::fmt::Display;

use colored::Colorize;
use forge_api::{Model, Provider};

/// Wrapper for displaying models in selection menus
///
/// This component provides consistent formatting for model selection across
/// the application, showing model ID with contextual information like
/// context length and tools support.
pub struct CliModel(pub Model);

impl Display for CliModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.id)?;

        let mut info_parts = Vec::new();

        // Add context length if available
        if let Some(limit) = self.0.context_length {
            if limit >= 1_000_000 {
                info_parts.push(format!("{}M", limit / 1_000_000));
            } else if limit >= 1000 {
                info_parts.push(format!("{}k", limit / 1000));
            } else {
                info_parts.push(format!("{limit}"));
            }
        }

        // Add tools support indicator if explicitly supported
        if self.0.tools_supported == Some(true) {
            info_parts.push("🛠️".to_string());
        }

        // Only show brackets if we have info to display
        if !info_parts.is_empty() {
            let info = format!("[ {} ]", info_parts.join(" "));
            write!(f, " {}", info.dimmed())?;
        }

        Ok(())
    }
}

/// Wrapper for displaying providers in selection menus
///
/// This component provides consistent formatting for provider selection across
/// the application, showing provider ID with domain information.
pub struct CliProvider(pub Provider);

impl Display for CliProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = self.0.id.to_string();
        write!(f, "{}", name)?;
        if let Some(domain) = self.0.url.domain() {
            write!(f, " [{}]", domain)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use console::strip_ansi_codes;
    use forge_api::{ModelId, ProviderId, ProviderResponse};
    use pretty_assertions::assert_eq;
    use url::Url;

    use super::*;

    fn create_model_fixture(
        id: &str,
        context_length: Option<u64>,
        tools_supported: Option<bool>,
    ) -> Model {
        Model {
            id: ModelId::new(id),
            name: None,
            description: None,
            context_length,
            tools_supported,
            supports_parallel_tool_calls: None,
            supports_reasoning: None,
        }
    }

    #[test]
    fn test_cli_model_display_with_context_and_tools() {
        let fixture = create_model_fixture("gpt-4", Some(128000), Some(true));
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "gpt-4 [ 128k 🛠️ ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_with_large_context() {
        let fixture = create_model_fixture("claude-3", Some(2000000), Some(true));
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "claude-3 [ 2M 🛠️ ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_with_small_context() {
        let fixture = create_model_fixture("small-model", Some(512), Some(false));
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "small-model [ 512 ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_with_context_only() {
        let fixture = create_model_fixture("text-model", Some(4096), Some(false));
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "text-model [ 4k ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_with_tools_only() {
        let fixture = create_model_fixture("tool-model", None, Some(true));
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "tool-model [ 🛠️ ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_empty_context_and_no_tools() {
        let fixture = create_model_fixture("basic-model", None, Some(false));
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "basic-model";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_empty_context_and_none_tools() {
        let fixture = create_model_fixture("unknown-model", None, None);
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "unknown-model";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_exact_thousands() {
        let fixture = create_model_fixture("exact-k", Some(8000), Some(true));
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "exact-k [ 8k 🛠️ ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_exact_millions() {
        let fixture = create_model_fixture("exact-m", Some(1000000), Some(true));
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "exact-m [ 1M 🛠️ ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_edge_case_999() {
        let fixture = create_model_fixture("edge-999", Some(999), None);
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "edge-999 [ 999 ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_model_display_edge_case_1001() {
        let fixture = create_model_fixture("edge-1001", Some(1001), None);
        let formatted = format!("{}", CliModel(fixture));
        let actual = strip_ansi_codes(&formatted);
        let expected = "edge-1001 [ 1k ]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_provider_display_minimal() {
        let fixture = Provider {
            id: ProviderId::OpenAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/").unwrap(),
            key: None,
        };
        let actual = format!("{}", CliProvider(fixture));
        let expected = "OpenAI [api.openai.com]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_provider_display_with_subdomain() {
        let fixture = Provider {
            id: ProviderId::OpenRouter,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://openrouter.ai/api/v1/").unwrap(),
            key: None,
        };
        let actual = format!("{}", CliProvider(fixture));
        let expected = "OpenRouter [openrouter.ai]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cli_provider_display_no_domain() {
        let fixture = Provider {
            id: ProviderId::Forge,
            response: ProviderResponse::OpenAI,
            url: Url::parse("http://localhost:8080/").unwrap(),
            key: None,
        };
        let actual = format!("{}", CliProvider(fixture));
        let expected = "Forge [localhost]";
        assert_eq!(actual, expected);
    }
}
