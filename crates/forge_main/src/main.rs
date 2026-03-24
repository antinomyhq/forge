use std::io::Read;
use std::panic;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use forge_api::ForgeAPI;
use forge_domain::TitleFormat;
use forge_main::{Cli, Sandbox, TitleDisplayExt, TopLevelCommand, UI, ZshCommandGroup, tracker};

#[tokio::main]
async fn main() -> Result<()> {
    // Install default rustls crypto provider (ring) before any TLS connections
    // This is required for rustls 0.23+ when multiple crypto providers are
    // available
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Set up panic hook for better error display
    panic::set_hook(Box::new(|panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unexpected error occurred".to_string()
        };

        println!("{}", TitleFormat::error(message.to_string()).display());
        tracker::error_blocking(message);
        std::process::exit(1);
    }));

    // Initialize and run the UI
    let mut cli = Cli::parse();

    // Fast path for `zsh rprompt` when no active conversation requires DB lookups.
    // This avoids heavy initialization (reqwest client, gRPC, tracing, 30+ service
    // objects) when all we need is to read a config file and some env vars.
    if matches!(
        cli.subcommands,
        Some(TopLevelCommand::Zsh(ZshCommandGroup::Rprompt))
    ) {
        let has_conversation = std::env::var("_FORGE_CONVERSATION_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .is_some();

        if !has_conversation {
            print!("{}", render_rprompt_fast());
            return Ok(());
        }
    }

    // Check if there's piped input
    if !atty::is(atty::Stream::Stdin) {
        let mut stdin_content = String::new();
        std::io::stdin().read_to_string(&mut stdin_content)?;
        let trimmed_content = stdin_content.trim();
        if !trimmed_content.is_empty() {
            cli.piped_input = Some(trimmed_content.to_string());
        }
    }

    // Handle worktree creation if specified
    let cwd: PathBuf = match (&cli.sandbox, &cli.directory) {
        (Some(sandbox), Some(cli)) => {
            let mut sandbox = Sandbox::new(sandbox).create()?;
            sandbox.push(cli);
            sandbox
        }
        (Some(sandbox), _) => Sandbox::new(sandbox).create()?,
        (_, Some(cli)) => match cli.canonicalize() {
            Ok(cwd) => cwd,
            Err(_) => panic!("Invalid path: {}", cli.display()),
        },
        (_, _) => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };

    // Initialize the ForgeAPI with the restricted mode if specified
    let restricted = cli.restricted;
    let cli_model = cli.model.clone();
    let cli_provider = cli.provider.clone();
    let mut ui = UI::init(cli, move || {
        ForgeAPI::init(
            restricted,
            cwd.clone(),
            cli_model.clone(),
            cli_provider.clone(),
        )
    })?;
    ui.run().await;

    Ok(())
}

/// Renders the ZSH rprompt without any heavy initialization.
/// Reads the config file directly and uses environment variables for all other state.
fn render_rprompt_fast() -> String {
    use forge_domain::{AgentId, AppConfig, ModelId};
    use forge_main::zsh::ZshRPrompt;

    // Read config to get the default model
    let model: Option<ModelId> = dirs::home_dir()
        .map(|home| home.join("forge").join(".config.json"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str::<AppConfig>(&content).ok())
        .and_then(|config| {
            let provider = config.provider?;
            config.model.get(&provider).cloned()
        });

    let agent = std::env::var("_FORGE_ACTIVE_AGENT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(AgentId::new);

    let use_nerd_font = std::env::var("NERD_FONT")
        .or_else(|_| std::env::var("USE_NERD_FONT"))
        .map(|val| val == "1")
        .unwrap_or(true);

    let currency_symbol =
        std::env::var("FORGE_CURRENCY_SYMBOL").unwrap_or_else(|_| "$".to_string());

    let conversion_ratio = std::env::var("FORGE_CURRENCY_CONVERSION_RATE")
        .ok()
        .and_then(|val| val.parse::<f64>().ok())
        .unwrap_or(1.0);

    ZshRPrompt::default()
        .agent(agent)
        .model(model)
        .use_nerd_font(use_nerd_font)
        .currency_symbol(currency_symbol)
        .conversion_ratio(conversion_ratio)
        .to_string()
}

#[cfg(test)]
mod tests {
    use forge_main::TopLevelCommand;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_stdin_detection_logic() {
        // This test verifies that the logic for detecting stdin is correct
        // We can't easily test the actual stdin reading in a unit test,
        // but we can verify the logic flow

        // Test that when prompt is provided, it remains independent of piped input
        let cli_with_prompt = Cli::parse_from(["forge", "--prompt", "existing prompt"]);
        let original_prompt = cli_with_prompt.prompt.clone();

        // The prompt should remain as provided
        assert_eq!(original_prompt, Some("existing prompt".to_string()));

        // Test that when no prompt is provided, piped_input field exists
        let cli_no_prompt = Cli::parse_from(["forge"]);
        assert_eq!(cli_no_prompt.prompt, None);
        assert_eq!(cli_no_prompt.piped_input, None);
    }

    #[test]
    fn test_cli_parsing_with_short_flag() {
        // Test that the short flag -p also works correctly
        let cli_with_short_prompt = Cli::parse_from(["forge", "-p", "short flag prompt"]);
        assert_eq!(
            cli_with_short_prompt.prompt,
            Some("short flag prompt".to_string())
        );
    }

    #[test]
    fn test_cli_parsing_other_flags_work_with_piping() {
        // Test that other CLI flags still work when expecting stdin input
        let cli_with_flags = Cli::parse_from(["forge", "--verbose", "--restricted"]);
        assert_eq!(cli_with_flags.prompt, None);
        assert_eq!(cli_with_flags.verbose, true);
        assert_eq!(cli_with_flags.restricted, true);
    }

    #[test]
    fn test_commit_command_diff_field_initially_none() {
        // Test that the diff field in CommitCommandGroup starts as None
        let cli = Cli::parse_from(["forge", "commit", "--preview"]);
        if let Some(TopLevelCommand::Commit(commit_group)) = cli.subcommands {
            assert_eq!(commit_group.preview, true);
            assert_eq!(commit_group.diff, None);
        } else {
            panic!("Expected Commit command");
        }
    }

    #[test]
    fn test_rprompt_fast_path_matches() {
        // Verify that `forge zsh rprompt` triggers the fast path match
        let cli = Cli::parse_from(["forge", "zsh", "rprompt"]);
        assert!(matches!(
            cli.subcommands,
            Some(TopLevelCommand::Zsh(ZshCommandGroup::Rprompt))
        ));
    }

    #[test]
    fn test_rprompt_fast_renders_without_config() {
        // Fast path should render successfully even when no config file exists.
        // With no agent/model/tokens it produces a dimmed default prompt.
        let output = render_rprompt_fast();
        assert!(!output.is_empty());
    }

    #[test]
    fn test_rprompt_fast_reads_config() {
        // Create a temporary config to verify the fast path reads it
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("forge");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join(".config.json");
        std::fs::write(
            &config_path,
            r#"{"provider":"anthropic","model":{"anthropic":"test-fast-model"}}"#,
        )
        .unwrap();

        // Override HOME to point to our temp dir so the fast path finds our config
        let original_home = std::env::var("HOME").ok();
        // SAFETY: this test is run serially (not concurrent with other tests that
        // depend on HOME)
        unsafe {
            std::env::set_var("HOME", dir.path());
        }

        let output = render_rprompt_fast();

        // Restore HOME
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            }
        }

        assert!(
            output.contains("test-fast-model"),
            "Expected rprompt to contain model from config, got: {}",
            output
        );
    }
}
