use std::io::{self, Write};

/// Terminal control utilities for managing terminal modes
pub struct TerminalControl;

impl TerminalControl {
    /// Disable bracketed paste mode
    ///
    /// Sends the escape sequence `\e[?2004l` to disable bracketed paste mode.
    /// This prevents terminals from wrapping pasted content with `~0` and `~1`
    /// markers.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to stdout fails or if flushing fails
    pub fn disable_bracketed_paste() -> io::Result<()> {
        let mut stdout = io::stdout();
        write!(stdout, "\x1b[?2004l")?;
        stdout.flush()
    }

    /// Enable bracketed paste mode
    ///
    /// Sends the escape sequence `\e[?2004h` to enable bracketed paste mode.
    /// This allows terminals to distinguish between typed and pasted content.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to stdout fails or if flushing fails
    pub fn enable_bracketed_paste() -> io::Result<()> {
        let mut stdout = io::stdout();
        write!(stdout, "\x1b[?2004h")?;
        stdout.flush()
    }

    /// Disable application cursor keys mode
    ///
    /// Sends the escape sequence `\e[?1l` to disable application cursor keys
    /// mode. This ensures arrow keys send standard sequences instead of
    /// application-specific ones.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to stdout fails or if flushing fails
    pub fn disable_application_cursor_keys() -> io::Result<()> {
        let mut stdout = io::stdout();
        write!(stdout, "\x1b[?1l")?;
        stdout.flush()
    }

    /// Enable application cursor keys mode
    ///
    /// Sends the escape sequence `\e[?1h` to enable application cursor keys
    /// mode. This makes arrow keys send application-specific sequences.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to stdout fails or if flushing fails
    pub fn enable_application_cursor_keys() -> io::Result<()> {
        let mut stdout = io::stdout();
        write!(stdout, "\x1b[?1h")?;
        stdout.flush()
    }
}

/// RAII guard that disables bracketed paste mode and re-enables it on drop
///
/// This is useful for ensuring bracketed paste mode is properly restored
/// even if an error occurs during execution.
///
/// # Examples
///
/// ```rust,ignore
/// use forge_select::BracketedPasteGuard;
///
/// {
///     let _guard = BracketedPasteGuard::new()?;
///     // Bracketed paste is now disabled
///     // ... do work that needs bracketed paste disabled ...
/// } // Bracketed paste is automatically re-enabled here
/// ```
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
/// This is useful for ensuring application cursor keys mode is properly
/// restored even if an error occurs during execution.
///
/// # Examples
///
/// ```rust,ignore
/// use forge_select::ApplicationCursorKeysGuard;
///
/// {
///     let _guard = ApplicationCursorKeysGuard::new()?;
///     // Application cursor keys are now disabled
///     // ... do work that needs standard arrow key sequences ...
/// } // Application cursor keys are automatically re-enabled here
/// ```
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


