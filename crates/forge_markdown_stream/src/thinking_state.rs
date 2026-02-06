//! Global state management for thinking blocks.
//!
//! Manages the collapsed/expanded state of thinking blocks and provides
//! functionality to toggle between states using keyboard shortcuts.

use std::sync::{Mutex, OnceLock};
use std::io::{self, Write};

/// Global thinking block state shared across all renderers
static THINKING_STATE: OnceLock<Mutex<ThinkingBlockState>> = OnceLock::new();

fn get_thinking_state() -> &'static Mutex<ThinkingBlockState> {
    THINKING_STATE.get_or_init(|| Mutex::new(ThinkingBlockState::default()))
}

/// State of the last rendered thinking block
#[derive(Debug, Default, Clone)]
pub struct ThinkingBlockState {
    /// The full content of the last thinking block (each line)
    pub lines: Vec<String>,
    /// Whether the thinking block is currently collapsed
    pub is_collapsed: bool,
    /// The line number where the collapsed indicator was written
    pub collapsed_line_position: Option<usize>,
}

impl ThinkingBlockState {
    /// Store a new thinking block
    pub fn store(lines: Vec<String>) {
        if let Ok(mut state) = get_thinking_state().lock() {
            state.lines = lines;
            state.is_collapsed = true;
            state.collapsed_line_position = None;
        }
    }
    
    /// Get the current thinking block content
    pub fn get() -> Option<Vec<String>> {
        get_thinking_state()
            .lock()
            .ok()
            .and_then(|state| {
                if state.lines.is_empty() {
                    None
                } else {
                    Some(state.lines.clone())
                }
            })
    }
    
    /// Check if currently collapsed
    pub fn is_collapsed() -> bool {
        get_thinking_state()
            .lock()
            .ok()
            .map(|state| state.is_collapsed)
            .unwrap_or(false)
    }
    
    /// Toggle between collapsed and expanded state
    pub fn toggle() -> io::Result<()> {
        let mut state = get_thinking_state()
            .lock()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        
        if state.lines.is_empty() {
            return Ok(());
        }
        
        state.is_collapsed = !state.is_collapsed;
        
        // Write to stdout to expand/collapse
        let mut stdout = io::stdout();
        
        if state.is_collapsed {
            // Collapse: move up, clear lines, write collapsed indicator
            let line_count = state.lines.len();
            write!(stdout, "\x1b[{}A", line_count)?;
            stdout.flush()?;
            
            for _ in 0..line_count {
                write!(stdout, "\r\x1b[K\x1b[B")?;
            }
            stdout.flush()?;
            
            write!(stdout, "\x1b[{}A", line_count)?;
            stdout.flush()?;
            
            writeln!(stdout, "▼ thinking (collapsed - press Option+L to expand)")?;
            stdout.flush()?;
        } else {
            // Expand: move up, clear collapsed line, write all lines
            write!(stdout, "\x1b[1A\r\x1b[K")?;
            stdout.flush()?;
            
            for line in &state.lines {
                writeln!(stdout, "{}", line)?;
            }
            stdout.flush()?;
        }
        
        Ok(())
    }
    
    /// Clear the stored thinking block
    pub fn clear() {
        if let Ok(mut state) = get_thinking_state().lock() {
            state.lines.clear();
            state.is_collapsed = false;
            state.collapsed_line_position = None;
        }
    }
}
