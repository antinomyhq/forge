//! Fish prompt styling utilities.
//!
//! This module provides helpers for generating ANSI escape sequences for Fish
//! shell prompts. Unlike ZSH which uses its own `%F{N}` prompt escapes, Fish
//! uses standard ANSI escape codes in its prompt functions.

use std::fmt::{self, Display};

/// ANSI 256-color code for Fish prompt styling.
#[derive(Debug, Clone, Copy)]
pub struct FishColor(u8);

impl FishColor {
    /// White (color 15)
    pub const WHITE: Self = Self(15);
    /// Cyan (color 134)
    pub const CYAN: Self = Self(134);
    /// Green (color 2)
    pub const GREEN: Self = Self(2);
    /// Dimmed gray (color 240)
    pub const DIMMED: Self = Self(240);

    /// Creates a color from a 256-color palette value.
    #[cfg(test)]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
}

impl Display for FishColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A styled string for Fish prompts using ANSI escape sequences.
#[derive(Debug, Clone)]
pub struct FishStyled<'a> {
    text: &'a str,
    fg: Option<FishColor>,
    bold: bool,
}

impl<'a> FishStyled<'a> {
    /// Creates a new styled string with the given text.
    pub fn new(text: &'a str) -> Self {
        Self { text, fg: None, bold: false }
    }

    /// Sets the foreground color.
    pub fn fg(mut self, color: FishColor) -> Self {
        self.fg = Some(color);
        self
    }

    /// Makes the text bold.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

impl Display for FishStyled<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let needs_reset = self.bold || self.fg.is_some();

        // Opening escapes
        if self.bold {
            write!(f, "\x1b[1m")?;
        }
        if let Some(ref color) = self.fg {
            write!(f, "\x1b[38;5;{}m", color)?;
        }

        // Text content
        write!(f, "{}", self.text)?;

        // Reset if any styling was applied
        if needs_reset {
            write!(f, "\x1b[0m")?;
        }

        Ok(())
    }
}

/// Extension trait for styling strings for Fish prompts.
pub trait FishStyle {
    /// Creates a Fish-styled wrapper for this string.
    fn fish(&self) -> FishStyled<'_>;
}

impl FishStyle for str {
    fn fish(&self) -> FishStyled<'_> {
        FishStyled::new(self)
    }
}

impl FishStyle for String {
    fn fish(&self) -> FishStyled<'_> {
        FishStyled::new(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let actual = "hello".fish().to_string();
        let expected = "hello";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bold() {
        let actual = "hello".fish().bold().to_string();
        let expected = "\x1b[1mhello\x1b[0m";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bold_and_color() {
        let actual = "hello".fish().bold().fg(FishColor::WHITE).to_string();
        let expected = "\x1b[1m\x1b[38;5;15mhello\x1b[0m";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fixed_color() {
        let actual = "hello".fish().fg(FishColor::new(240)).to_string();
        let expected = "\x1b[38;5;240mhello\x1b[0m";
        assert_eq!(actual, expected);
    }
}
