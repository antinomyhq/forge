use std::env;
use std::io::IsTerminal;
use std::str::FromStr;

use colored::ColoredString;

/// Color mode configuration for terminal output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Always use colors
    Always,
    /// Use colors only when output is to a terminal (auto-detect)
    Auto,
    /// Never use colors
    Never,
}

impl Default for ColorMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl FromStr for ColorMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "always" | "true" | "1" | "yes" => Ok(Self::Always),
            "auto" => Ok(Self::Auto),
            "never" | "false" | "0" | "no" => Ok(Self::Never),
            _ => Err(format!(
                "Invalid color mode: {s}. Expected: always, auto, never"
            )),
        }
    }
}

/// Global color configuration
#[derive(Debug, Clone)]
pub struct ColorConfig {
    mode: ColorMode,
    is_terminal: bool,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorConfig {
    /// Create a new color configuration with auto-detection
    pub fn new() -> Self {
        let is_terminal = std::io::stdout().is_terminal();
        let mode = Self::detect_color_mode();

        Self { mode, is_terminal }
    }

    /// Create a color configuration with explicit mode
    pub fn with_mode(mode: ColorMode) -> Self {
        let is_terminal = std::io::stdout().is_terminal();
        Self { mode, is_terminal }
    }

    /// Detect color mode from environment variables and CLI flags
    /// Priority: CLI flag > FORGE_COLOR > NO_COLOR > default (auto)
    fn detect_color_mode() -> ColorMode {
        // Check NO_COLOR first (industry standard)
        if let Ok(no_color) = env::var("NO_COLOR")
            && !no_color.is_empty() {
                return ColorMode::Never;
            }

        // Check FORGE_COLOR environment variable
        if let Ok(forge_color) = env::var("FORGE_COLOR")
            && let Ok(mode) = ColorMode::from_str(&forge_color) {
                return mode;
            }

        // Default to auto-detection
        ColorMode::Auto
    }

    /// Check if colors should be used based on configuration and terminal
    /// detection
    pub fn should_use_color(&self) -> bool {
        match self.mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => self.is_terminal,
        }
    }

    /// Get the current color mode
    pub fn mode(&self) -> ColorMode {
        self.mode
    }

    /// Set the color mode (useful for CLI flag override)
    pub fn set_mode(&mut self, mode: ColorMode) {
        self.mode = mode;
    }

    /// Apply color if colors are enabled, otherwise return plain text
    pub fn colorize<F>(&self, text: &str, color_fn: F) -> String
    where
        F: FnOnce(&str) -> ColoredString,
    {
        if self.should_use_color() {
            color_fn(text).to_string()
        } else {
            text.to_string()
        }
    }
}

/// Global color configuration instance
static GLOBAL_COLOR_CONFIG: std::sync::OnceLock<ColorConfig> = std::sync::OnceLock::new();

/// Initialize the global color configuration
pub fn init_color_config(config: ColorConfig) {
    GLOBAL_COLOR_CONFIG.set(config).ok();
}

/// Get the global color configuration
pub fn color_config() -> &'static ColorConfig {
    GLOBAL_COLOR_CONFIG.get_or_init(ColorConfig::new)
}

/// Convenience functions for common color operations
pub fn colorize_if_enabled<F>(text: &str, color_fn: F) -> String
where
    F: FnOnce(&str) -> ColoredString,
{
    color_config().colorize(text, color_fn)
}

/// Enhanced color functions with better contrast for light backgrounds
pub mod enhanced {
    use colored::Colorize;

    use super::colorize_if_enabled;

    /// Yellow that works better on light backgrounds
    pub fn yellow(text: &str) -> String {
        colorize_if_enabled(text, |s| s.truecolor(184, 134, 11)) // Darker yellow
    }

    /// White that works better on light backgrounds  
    pub fn white(text: &str) -> String {
        colorize_if_enabled(text, |s| s.truecolor(55, 65, 81)) // Dark gray instead of white
    }

    /// Dimmed text that works better on light backgrounds
    pub fn dimmed(text: &str) -> String {
        colorize_if_enabled(text, |s| s.truecolor(107, 114, 128)) // Medium gray
    }

    /// Red that works on both light and dark backgrounds
    pub fn red(text: &str) -> String {
        colorize_if_enabled(text, |s| s.truecolor(220, 38, 127)) // Vivid red
    }

    /// Green that works on both light and dark backgrounds
    pub fn green(text: &str) -> String {
        colorize_if_enabled(text, |s| s.truecolor(34, 197, 94)) // Vivid green
    }

    /// Cyan that works on both light and dark backgrounds
    pub fn cyan(text: &str) -> String {
        colorize_if_enabled(text, |s| s.truecolor(6, 182, 212)) // Vivid cyan
    }

    /// Bold text
    pub fn bold(text: &str) -> String {
        colorize_if_enabled(text, |s| s.bold())
    }

    /// Bright cyan for emphasis
    pub fn bright_cyan(text: &str) -> String {
        colorize_if_enabled(text, |s| s.truecolor(34, 211, 238)) // Bright cyan
    }
}

#[cfg(test)]
mod tests {
    use colored::Colorize;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_color_mode_from_str() {
        let fixtures = vec![
            ("always", ColorMode::Always),
            ("true", ColorMode::Always),
            ("1", ColorMode::Always),
            ("yes", ColorMode::Always),
            ("auto", ColorMode::Auto),
            ("never", ColorMode::Never),
            ("false", ColorMode::Never),
            ("0", ColorMode::Never),
            ("no", ColorMode::Never),
        ];

        for (input, expected) in fixtures {
            let actual = ColorMode::from_str(input).unwrap();
            assert_eq!(actual, expected, "Failed for input: {input}");
        }
    }

    #[test]
    fn test_color_mode_from_str_invalid() {
        let fixture = "invalid";
        let actual = ColorMode::from_str(fixture);
        assert!(actual.is_err());
    }

    #[test]
    fn test_color_config_always_mode() {
        let fixture = ColorConfig::with_mode(ColorMode::Always);
        let actual = fixture.should_use_color();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_color_config_never_mode() {
        let fixture = ColorConfig::with_mode(ColorMode::Never);
        let actual = fixture.should_use_color();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_colorize_with_colors_enabled() {
        let fixture = ColorConfig::with_mode(ColorMode::Always);
        let actual = fixture.colorize("test", |s| s.red());
        // Should contain ANSI escape codes
        assert!(actual.contains("\x1b["));
    }

    #[test]
    fn test_colorize_with_colors_disabled() {
        let fixture = ColorConfig::with_mode(ColorMode::Never);
        let actual = fixture.colorize("test", |s| s.red());
        let expected = "test";
        assert_eq!(actual, expected);
    }
}
