//! ZSH shell integration.
//!
//! This module provides all ZSH-related functionality including:
//! - Plugin generation and installation
//! - Theme generation
//! - Shell diagnostics
//! - Right prompt (rprompt) display
//! - Prompt styling utilities

mod plugin;
mod rprompt;
mod style;

/// Re-export from shared shell module for backward compatibility.
pub(crate) use crate::shell::normalize_script;

pub use plugin::{
    generate_zsh_plugin, generate_zsh_theme, run_zsh_doctor, run_zsh_keyboard,
    setup_zsh_integration,
};
pub use rprompt::ZshRPrompt;
