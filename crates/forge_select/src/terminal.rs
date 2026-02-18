use std::io::{self, stdout};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::cursor::Show;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::{Command, execute};

static SIGNAL_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Restore cursor visibility using raw ANSI escape code
///
/// This is more reliable than crossterm when the program is exiting
fn restore_cursor() {
    use std::io::Write;
    let _ = std::io::stdout().write_all(b"\x1b[?25h");
    let _ = std::io::stdout().flush();
}

/// Install global signal handler to restore cursor on Ctrl+C
///
/// This ensures cursor visibility is restored even if dialoguer is interrupted.
/// Only installs the handler once, subsequent calls are no-ops.
///
/// # Errors
///
/// Returns an error if the signal handler cannot be installed
pub fn install_signal_handler() -> io::Result<()> {
    // Only install once using atomic compare-and-swap
    if SIGNAL_HANDLER_INSTALLED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        ctrlc::set_handler(move || {
            restore_cursor();
            std::process::exit(130); // 128 + SIGINT(2) = 130
        })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    }
    Ok(())
}

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

    /// Show the terminal cursor
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

/// RAII guard that ensures the cursor is shown when dropped
///
/// This guard restores cursor visibility when it goes out of scope,
/// ensuring the cursor is shown even if an error occurs during execution.
#[derive(Default)]
pub struct CursorRestoreGuard;

impl Drop for CursorRestoreGuard {
    fn drop(&mut self) {
        // Best effort to re-enable - ignore errors during drop
        let _ = TerminalControl::show_cursor();
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
