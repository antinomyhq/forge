//! Utility functions for the markdown renderer.

/// Terminal theme mode (dark or light).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    /// Dark terminal background.
    Dark,
    /// Light terminal background.
    Light,
}

/// Detects the terminal theme mode (dark or light).
pub fn detect_theme_mode() -> ThemeMode {
    use terminal_colorsaurus::{theme_mode, QueryOptions};
    use terminal_colorsaurus::ThemeMode as ColorsaurusThemeMode;

    match theme_mode(QueryOptions::default()) {
        Ok(ColorsaurusThemeMode::Light) => ThemeMode::Light,
        Ok(ColorsaurusThemeMode::Dark) | Err(_) => ThemeMode::Dark,
    }
}
