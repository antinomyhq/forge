//! ZSH shell integration.
//!
//! This module provides all ZSH-related functionality including:
//! - Plugin generation and installation
//! - Theme generation
//! - Shell diagnostics
//! - Right prompt (rprompt) display
//! - Prompt styling utilities
//! - Full setup orchestration (zsh, Oh My Zsh, plugins)

mod plugin;
mod rprompt;
mod setup;
mod style;

/// Normalizes shell script content for cross-platform compatibility.
///
/// Strips carriage returns (`\r`) that appear when `include_str!` or
/// `include_dir!` embed files on Windows (where `git core.autocrlf=true`
/// converts LF to CRLF on checkout). Zsh cannot parse `\r` in scripts.
pub(crate) fn normalize_script(content: &str) -> String {
    content.replace('\r', "")
}

pub use plugin::{
    generate_zsh_plugin, generate_zsh_theme, run_zsh_doctor, run_zsh_keyboard,
    setup_zsh_integration,
};
pub use rprompt::ZshRPrompt;
pub use setup::{
    BatStatus, FdStatus, FzfStatus, OmzStatus, Platform, PluginStatus, ZshStatus,
    configure_bashrc_autostart, detect_all_dependencies, detect_git, detect_platform, detect_sudo,
    install_autosuggestions, install_bat, install_fd, install_fzf, install_oh_my_zsh,
    install_syntax_highlighting, install_zsh, resolve_command_path,
};
