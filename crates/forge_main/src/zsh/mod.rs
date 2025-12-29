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

pub use plugin::{generate_zsh_plugin, generate_zsh_theme, run_zsh_doctor, setup_zsh_integration};
pub use rprompt::ZshRPrompt;
