//! Fish shell integration.
//!
//! This module provides all Fish-related functionality including:
//! - Plugin generation and installation
//! - Theme generation
//! - Shell diagnostics
//! - Right prompt (rprompt) display
//! - Prompt styling utilities

mod plugin;
mod rprompt;
mod style;

pub use plugin::{
    generate_fish_plugin, generate_fish_theme, run_fish_doctor, run_fish_keyboard,
    setup_fish_integration,
};
pub use rprompt::FishRPrompt;
