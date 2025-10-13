use std::path::{Path, PathBuf};

use anyhow::Context;
use colored::Colorize;
use forge_domain::{Banner, TitleFormat};
use forge_tracker::VERSION;

use crate::title_display::TitleDisplayExt;

const DEFAULT_BANNER: &str = include_str!("banner");

/// Banner configuration (CLI layer)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BannerConfig {
    /// Use the default built-in banner
    Default,
    /// Disable banner display
    Disable,
    /// Load custom banner from file
    Custom(PathBuf),
}

impl std::str::FromStr for BannerConfig {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("disable") {
            Ok(Self::Disable)
        } else {
            Ok(Self::Custom(PathBuf::from(s)))
        }
    }
}

impl From<&Banner> for BannerConfig {
    fn from(banner: &Banner) -> Self {
        match banner {
            Banner::Disable => Self::Disable,
            Banner::Custom(path) => Self::Custom(path.clone()),
        }
    }
}

/// Display banner based on configuration
///
/// Loads and displays banner content. If custom banner fails to load, logs
/// error and falls back to default banner.
pub async fn display(interactive: bool, config: Option<BannerConfig>) -> anyhow::Result<()> {
    let config = config.unwrap_or(BannerConfig::Default);

    match load(&config, interactive).await {
        Ok(Some(content)) => println!("{content}"),
        Ok(None) => {}
        Err(err) => {
            let warning = TitleFormat::error("Banner Error")
                .sub_title(format!("{err}. Falling back to default banner"))
                .display();
            println!("{warning}");
            if let Some(content) = load(&BannerConfig::Default, interactive).await? {
                println!("{content}");
            }
        }
    }

    Ok(())
}

async fn load(config: &BannerConfig, interactive: bool) -> anyhow::Result<Option<String>> {
    match config {
        BannerConfig::Disable => Ok(None),
        BannerConfig::Default => Ok(Some(format_banner(DEFAULT_BANNER, interactive))),
        BannerConfig::Custom(path) => {
            let content = read_file(path).await?;
            Ok(Some(format_banner(&content, interactive)))
        }
    }
}

fn format_banner(raw: &str, interactive: bool) -> String {
    let tips = if interactive {
        [
            ("New conversation:", "/new"),
            ("Get started:", "/info, /usage, /help, /conversation"),
            ("Switch model:", "/model"),
            ("Switch agent:", "/forge or /muse or /agent"),
            ("Update:", "/update"),
            ("Quit:", "/exit or <CTRL+D>"),
        ]
        .as_slice()
    } else {
        [
            ("New conversation:", ":new"),
            ("Get started:", ":info, :conversation"),
            ("Switch model:", ":model"),
            ("Switch provider:", ":provider"),
            ("Switch agent:", ":<agent_name> e.g. :forge or :muse"),
        ]
        .as_slice()
    };

    let max_width = tips.iter().map(|(k, _)| k.len()).max().unwrap_or(0);

    let mut banner = raw.to_string();
    banner.push_str(&format!(
        "\n{}{}",
        format!("{:>max_width$} ", "Version:").dimmed(),
        VERSION.cyan()
    ));

    for (key, value) in tips {
        banner.push_str(&format!(
            "\n{}{}",
            format!("{key:>max_width$} ").dimmed(),
            value.cyan()
        ));
    }

    banner.push('\n');
    banner
}

async fn read_file(path: &Path) -> anyhow::Result<String> {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    anyhow::ensure!(
        resolved.exists(),
        "Custom banner file not found: {}",
        resolved.display()
    );

    let content = tokio::fs::read_to_string(&resolved)
        .await
        .with_context(|| format!("Failed to read custom banner from {}", resolved.display()))?;

    anyhow::ensure!(
        !content.trim().is_empty(),
        "Custom banner file is empty: {}",
        resolved.display()
    );

    Ok(content)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_load_disable() {
        let actual = load(&BannerConfig::Disable, false).await.unwrap();
        assert_eq!(actual, None);
    }

    #[tokio::test]
    async fn test_load_default_non_interactive() {
        let actual = load(&BannerConfig::Default, false).await.unwrap().unwrap();
        insta::assert_snapshot!(strip_ansi_escapes::strip_str(actual));
    }

    #[tokio::test]
    async fn test_load_default_interactive() {
        let actual = load(&BannerConfig::Default, true).await.unwrap().unwrap();
        insta::assert_snapshot!(strip_ansi_escapes::strip_str(actual));
    }

    #[tokio::test]
    async fn test_load_custom() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("banner.txt");
        fs::write(&path, "Custom Banner\n").unwrap();

        let actual = load(&BannerConfig::Custom(path), false)
            .await
            .unwrap()
            .unwrap();
        insta::assert_snapshot!(strip_ansi_escapes::strip_str(actual));
    }

    #[tokio::test]
    async fn test_load_custom_not_found() {
        let result = load(
            &BannerConfig::Custom(PathBuf::from("/nonexistent.txt")),
            false,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_display_falls_back_on_error() {
        let config = Some(BannerConfig::Custom(PathBuf::from("/nonexistent.txt")));
        let result = display(false, config).await;
        assert!(result.is_ok()); // Should succeed by falling back to default
    }
}
