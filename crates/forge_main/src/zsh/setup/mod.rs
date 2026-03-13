//! ZSH setup orchestrator for `forge zsh setup`.
//!
//! Detects and installs all dependencies required for forge's shell
//! integration: zsh, Oh My Zsh, zsh-autosuggestions, zsh-syntax-highlighting.
//! Handles platform-specific installation (Linux, macOS, Android, Windows/Git
//! Bash) with parallel dependency detection and installation where possible.
//!
//! # Module layout
//!
//! | Module             | Responsibility |
//! |--------------------|----------------|
//! | `platform`         | OS detection (`Platform`, `detect_platform`) |
//! | `libc`             | C-library detection (`LibcType`, `detect_libc_type`) |
//! | `types`            | Status enums (`ZshStatus`, `FzfStatus`, …, `DependencyStatus`) |
//! | `util`             | Path / command helpers, `version_gte`, sudo runner |
//! | `detect`           | Dependency detection (`detect_all_dependencies`, per-tool) |
//! | `install_zsh`      | ZSH + zshenv installation (per platform) |
//! | `install_plugins`  | Oh My Zsh, zsh-autosuggestions, zsh-syntax-highlighting, bash_profile |
//! | `install_tools`    | fzf / bat / fd (package manager + GitHub fallback) |

mod detect;
mod install_plugins;
mod install_tools;
mod install_zsh;
mod libc;
mod platform;
mod types;
mod util;
mod installer;
// ── Constants (shared across submodules) ─────────────────────────────────────

/// Base URL for MSYS2 package repository.
pub(super) const MSYS2_BASE: &str = "https://repo.msys2.org/msys/x86_64";

/// Package names required for ZSH on MSYS2/Windows.
pub(super) const MSYS2_PKGS: &[&str] = &[
    "zsh",
    "ncurses",
    "libpcre2_8",
    "libiconv",
    "libgdbm",
    "gcc-libs",
];

/// URL for the Oh My Zsh install script.
pub(super) const OMZ_INSTALL_URL: &str =
    "https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh";

/// Minimum acceptable fzf version.
pub(super) const FZF_MIN_VERSION: &str = "0.36.0";

/// Minimum acceptable bat version.
pub(super) const BAT_MIN_VERSION: &str = "0.20.0";

/// Minimum acceptable fd version.
pub(super) const FD_MIN_VERSION: &str = "10.0.0";

// ── Public re-exports ────────────────────────────────────────────────────────
//
// These items are the **only** public surface of the `setup` module and must
// match exactly what `zsh/mod.rs` imports via `pub use setup::{…}`.

pub use detect::{detect_all_dependencies, detect_git, detect_sudo};
pub use install_plugins::{
    ConfigureBashProfile, InstallAutosuggestions, InstallOhMyZsh, InstallSyntaxHighlighting,
};
pub use install_tools::{InstallBat, InstallFd, InstallFzf};
pub use install_zsh::InstallZsh;
pub use installer::{Group, Installation, Installer, Noop, Task};
pub use platform::{Platform, detect_platform};
pub use types::{
    BatStatus, DependencyStatus, FdStatus, FzfStatus, OmzStatus, PluginStatus, SudoCapability,
    ZshStatus,
};
pub use util::resolve_command_path;
