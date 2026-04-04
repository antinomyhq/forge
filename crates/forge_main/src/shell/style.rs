//! ANSI escape code styling utilities for shell prompts.
//!
//! Provides helpers for generating ANSI escape sequences, used by shells
//! that support standard ANSI codes (PowerShell, fish, nushell, etc.) as
//! opposed to shell-specific escapes (zsh's `%F{N}`).
//!
//! Uses basic 4-bit ANSI color codes (30-37, 90-97) for maximum
//! compatibility with Windows PowerShell 5.1 and older terminals.

use std::fmt::{self, Display};

/// Basic ANSI foreground color code (4-bit, universally supported).
///
/// Uses standard codes (30-37) and bright codes (90-97) instead of
/// 256-color `38;5;N` which is not supported by Windows PowerShell 5.1.
#[derive(Debug, Clone, Copy)]
pub struct AnsiColor(u8);

impl AnsiColor {
    /// Bright white (code 97)
    pub const WHITE: Self = Self(97);
    /// Cyan (code 36)
    pub const CYAN: Self = Self(36);
    /// Green (code 32)
    pub const GREEN: Self = Self(32);
    /// Dark gray / dimmed (code 90)
    pub const DIMMED: Self = Self(90);
}

/// A styled string using ANSI escape codes.
#[derive(Debug, Clone)]
pub struct AnsiStyled<'a> {
    text: &'a str,
    fg: Option<AnsiColor>,
    bold: bool,
}

impl<'a> AnsiStyled<'a> {
    /// Creates a new styled string with the given text.
    pub fn new(text: &'a str) -> Self {
        Self { text, fg: None, bold: false }
    }

    /// Sets the foreground color.
    pub fn fg(mut self, color: AnsiColor) -> Self {
        self.fg = Some(color);
        self
    }

    /// Makes the text bold.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

impl Display for AnsiStyled<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let has_style = self.bold || self.fg.is_some();

        if has_style {
            write!(f, "\x1b[")?;
            let mut first = true;

            if self.bold {
                write!(f, "1")?;
                first = false;
            }

            if let Some(ref color) = self.fg {
                if !first {
                    write!(f, ";")?;
                }
                write!(f, "{}", color.0)?;
            }

            write!(f, "m")?;
        }

        write!(f, "{}", self.text)?;

        if has_style {
            write!(f, "\x1b[0m")?;
        }

        Ok(())
    }
}

/// Extension trait for styling strings with ANSI escape codes.
pub trait AnsiStyle {
    /// Creates an ANSI-styled wrapper for this string.
    fn ansi(&self) -> AnsiStyled<'_>;
}

impl AnsiStyle for str {
    fn ansi(&self) -> AnsiStyled<'_> {
        AnsiStyled::new(self)
    }
}

impl AnsiStyle for String {
    fn ansi(&self) -> AnsiStyled<'_> {
        AnsiStyled::new(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let actual = "hello".ansi().to_string();
        assert_eq!(actual, "hello");
    }

    #[test]
    fn test_bold() {
        let actual = "hello".ansi().bold().to_string();
        assert_eq!(actual, "\x1b[1mhello\x1b[0m");
    }

    #[test]
    fn test_color() {
        let actual = "hello".ansi().fg(AnsiColor::DIMMED).to_string();
        assert_eq!(actual, "\x1b[90mhello\x1b[0m");
    }

    #[test]
    fn test_bold_and_color() {
        let actual = "hello".ansi().bold().fg(AnsiColor::WHITE).to_string();
        assert_eq!(actual, "\x1b[1;97mhello\x1b[0m");
    }
}
