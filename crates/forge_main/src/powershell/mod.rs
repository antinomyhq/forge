//! PowerShell shell integration.
//!
//! This module provides PowerShell-specific functionality including:
//! - Plugin generation and installation
//! - Theme generation
//! - Shell diagnostics
//! - Right prompt (rprompt) display using ANSI escape codes

mod plugin;
mod rprompt;

pub use plugin::{
    generate_powershell_plugin, generate_powershell_theme, run_powershell_doctor,
    run_powershell_keyboard, setup_powershell_integration,
};
pub use rprompt::PowerShellRPrompt;
