use std::io::{self, stdout};

use crossterm::cursor::Show;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::{Command, execute};

/// Terminal control utilities for managing terminal modes
pub struct TerminalControl;

impl TerminalControl {
    /// Disable bracketed paste mode
    ///
    /// Prevents terminals from wrapping pasted content with special markers.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal command fails to execute
    pub fn disable_bracketed_paste() -> io::Result<()> {
        execute!(stdout(), DisableBracketedPaste)
    }

    /// Enable bracketed paste mode
    ///
    /// Allows terminals to distinguish between typed and pasted content.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal command fails to execute
    pub fn enable_bracketed_paste() -> io::Result<()> {
        execute!(stdout(), EnableBracketedPaste)
    }

    /// Disable application cursor keys mode
    ///
    /// Ensures arrow keys send standard sequences instead of
    /// application-specific ones.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal command fails to execute
    pub fn disable_application_cursor_keys() -> io::Result<()> {
        execute!(stdout(), DisableApplicationCursorKeys)
    }

    /// Enable application cursor keys mode
    ///
    /// Makes arrow keys send application-specific sequences.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal command fails to execute
    pub fn enable_application_cursor_keys() -> io::Result<()> {
        execute!(stdout(), EnableApplicationCursorKeys)
    }

    /// Show cursor
    ///
    /// Makes the cursor visible in the terminal.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal command fails to execute
    pub fn show_cursor() -> io::Result<()> {
        execute!(stdout(), Show)
    }
}

/// RAII guard that disables bracketed paste mode and re-enables it on drop
///
/// This ensures bracketed paste mode is properly restored even if an error
/// occurs during execution.
pub struct BracketedPasteGuard {
    _private: (),
}

impl BracketedPasteGuard {
    /// Create a new guard, disabling bracketed paste mode
    ///
    /// # Errors
    ///
    /// Returns an error if disabling bracketed paste mode fails
    pub fn new() -> io::Result<Self> {
        TerminalControl::disable_bracketed_paste()?;
        Ok(Self { _private: () })
    }
}

impl Drop for BracketedPasteGuard {
    fn drop(&mut self) {
        // Best effort to re-enable - ignore errors during drop
        let _ = TerminalControl::enable_bracketed_paste();
    }
}

/// RAII guard that disables application cursor keys mode and re-enables it on
/// drop
///
/// This ensures application cursor keys mode is properly restored even if an
/// error occurs during execution.
pub struct ApplicationCursorKeysGuard {
    _private: (),
}

impl ApplicationCursorKeysGuard {
    /// Create a new guard, disabling application cursor keys mode
    ///
    /// # Errors
    ///
    /// Returns an error if disabling application cursor keys mode fails
    pub fn new() -> io::Result<Self> {
        TerminalControl::disable_application_cursor_keys()?;
        Ok(Self { _private: () })
    }
}

impl Drop for ApplicationCursorKeysGuard {
    fn drop(&mut self) {
        // Best effort to re-enable - ignore errors during drop
        let _ = TerminalControl::enable_application_cursor_keys();
    }
}

/// RAII guard that ensures cursor is visible on Ctrl+C
///
/// This guard sets up a global Ctrl+C handler that restores cursor visibility
/// before exiting. This prevents the cursor from remaining hidden if the user
/// interrupts a prompt with Ctrl+C.
///
/// The `ctrlc` crate internally ensures the handler is only set once, so
/// creating multiple guards is safe and will reuse the same handler.
pub struct CursorRestoreGuard {
    _private: (),
}

impl CursorRestoreGuard {
    /// Create a new guard and ensure Ctrl+C handler is set
    ///
    /// Uses `ctrlc::try_set_handler` which safely handles multiple calls by
    /// returning an error if a handler is already set. The error is ignored
    /// since we only care that a handler exists, not who set it.
    pub fn new() -> Self {
        let _ = ctrlc::try_set_handler(|| {
            let _ = TerminalControl::show_cursor();
            std::process::exit(130);
        });
        Self { _private: () }
    }
}

/// Custom crossterm command to disable application cursor keys mode
///
/// Sends the DECCKM escape sequence to disable application cursor keys.
struct DisableApplicationCursorKeys;

impl Command for DisableApplicationCursorKeys {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "\x1b[?1l")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "not supported on Windows, using ANSI fallback",
        ))
    }
}

/// Custom crossterm command to enable application cursor keys mode
///
/// Sends the DECCKM escape sequence to enable application cursor keys.
struct EnableApplicationCursorKeys;

impl Command for EnableApplicationCursorKeys {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "\x1b[?1h")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "not supported on Windows, using ANSI fallback",
        ))
    }
}
