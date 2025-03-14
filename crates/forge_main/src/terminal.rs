use std::io::{self, Result as IoResult};
use std::sync::Mutex;

use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use lazy_static::lazy_static;

lazy_static! {
    static ref TERMINAL_MANAGER: Mutex<TerminalManager> = Mutex::new(TerminalManager::new());
}

/// A wrapper around terminal state management functions
pub struct TerminalManager {
    is_alt_screen: bool,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self { is_alt_screen: false }
    }

    /// Save the current terminal state and set up the terminal for the
    /// application
    pub fn save_and_setup(&mut self) -> IoResult<()> {
        execute!(io::stdout(), EnterAlternateScreen)?;
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, 0))?;
        self.is_alt_screen = true;

        Ok(())
    }

    /// Restore the terminal to its original state
    pub fn restore(&mut self) -> IoResult<()> {
        if self.is_alt_screen {
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            self.is_alt_screen = false;
        }

        Ok(())
    }
}

/// Save the terminal state and set it up for the application
pub fn setup_terminal() -> Result<()> {
    let mut manager = TERMINAL_MANAGER.lock().unwrap();
    manager.save_and_setup()?;
    Ok(())
}

/// Restore the terminal to its original state
pub fn restore_terminal() -> Result<()> {
    let mut manager = TERMINAL_MANAGER.lock().unwrap();
    manager.restore()?;
    Ok(())
}

/// A guard that will automatically restore the terminal on drop
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Err(err) = restore_terminal() {
            eprintln!("Error restoring terminal: {:?}", err);
        }
    }
}

/// Initialize the terminal and return a guard that will restore it on drop
pub fn initialize() -> Result<TerminalGuard> {
    setup_terminal()?;
    Ok(TerminalGuard)
}
