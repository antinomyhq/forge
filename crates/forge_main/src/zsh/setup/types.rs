//! Dependency status types for the ZSH setup orchestrator.
//!
//! Pure data types representing the installation status of each dependency
//! (zsh, Oh My Zsh, plugins, fzf, bat, fd) and related capability enums.

use std::path::PathBuf;

/// Status of the zsh shell installation.
#[derive(Debug, Clone)]
pub enum ZshStatus {
    /// zsh was not found on the system.
    NotFound,
    /// zsh was found but modules are broken (needs reinstall).
    Broken {
        /// Path to the zsh binary
        path: String,
    },
    /// zsh is installed and fully functional.
    Functional {
        /// Detected version string (e.g., "5.9")
        version: String,
        /// Path to the zsh binary
        path: String,
    },
}

/// Status of Oh My Zsh installation.
#[derive(Debug, Clone)]
pub enum OmzStatus {
    /// Oh My Zsh is not installed.
    NotInstalled,
    /// Oh My Zsh is installed at the given path.
    Installed {
        /// Path to the Oh My Zsh directory
        #[allow(dead_code)]
        path: PathBuf,
    },
}

/// Status of a zsh plugin (autosuggestions or syntax-highlighting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    /// Plugin is not installed.
    NotInstalled,
    /// Plugin is installed.
    Installed,
}

/// Status of fzf installation.
#[derive(Debug, Clone)]
pub enum FzfStatus {
    /// fzf was not found.
    NotFound,
    /// fzf was found with the given version. `meets_minimum` indicates whether
    /// it meets the minimum required version.
    Found {
        /// Detected version string
        version: String,
        /// Whether the version meets the minimum requirement
        meets_minimum: bool,
    },
}

/// Status of bat installation.
#[derive(Debug, Clone)]
pub enum BatStatus {
    /// bat was not found.
    NotFound,
    /// bat is installed.
    Installed {
        /// Detected version string
        version: String,
        /// Whether the version meets the minimum requirement (0.20.0+)
        meets_minimum: bool,
    },
}

/// Status of fd installation.
#[derive(Debug, Clone)]
pub enum FdStatus {
    /// fd was not found.
    NotFound,
    /// fd is installed.
    Installed {
        /// Detected version string
        version: String,
        /// Whether the version meets the minimum requirement (10.0.0+)
        meets_minimum: bool,
    },
}

/// Aggregated dependency detection results.
#[derive(Debug, Clone)]
pub struct DependencyStatus {
    /// Status of zsh installation
    pub zsh: ZshStatus,
    /// Status of Oh My Zsh installation
    pub oh_my_zsh: OmzStatus,
    /// Status of zsh-autosuggestions plugin
    pub autosuggestions: PluginStatus,
    /// Status of zsh-syntax-highlighting plugin
    pub syntax_highlighting: PluginStatus,
    /// Status of fzf installation
    pub fzf: FzfStatus,
    /// Status of bat installation
    pub bat: BatStatus,
    /// Status of fd installation
    pub fd: FdStatus,
    /// Whether git is available (hard prerequisite)
    #[allow(dead_code)]
    pub git: bool,
}

impl DependencyStatus {
    /// Returns true if all required dependencies are installed and functional.
    pub fn all_installed(&self) -> bool {
        matches!(self.zsh, ZshStatus::Functional { .. })
            && matches!(self.oh_my_zsh, OmzStatus::Installed { .. })
            && self.autosuggestions == PluginStatus::Installed
            && self.syntax_highlighting == PluginStatus::Installed
    }

    /// Returns a list of human-readable names for items that need to be
    /// installed.
    pub fn missing_items(&self) -> Vec<(&'static str, &'static str)> {
        let mut items = Vec::new();
        if !matches!(self.zsh, ZshStatus::Functional { .. }) {
            items.push(("zsh", "shell"));
        }
        if !matches!(self.oh_my_zsh, OmzStatus::Installed { .. }) {
            items.push(("Oh My Zsh", "plugin framework"));
        }
        if self.autosuggestions == PluginStatus::NotInstalled {
            items.push(("zsh-autosuggestions", "plugin"));
        }
        if self.syntax_highlighting == PluginStatus::NotInstalled {
            items.push(("zsh-syntax-highlighting", "plugin"));
        }
        if matches!(self.fzf, FzfStatus::NotFound) {
            items.push(("fzf", "fuzzy finder"));
        }
        if matches!(self.bat, BatStatus::NotFound) {
            items.push(("bat", "file viewer"));
        }
        if matches!(self.fd, FdStatus::NotFound) {
            items.push(("fd", "file finder"));
        }
        items
    }

    /// Returns true if zsh needs to be installed.
    pub fn needs_zsh(&self) -> bool {
        !matches!(self.zsh, ZshStatus::Functional { .. })
    }

    /// Returns true if Oh My Zsh needs to be installed.
    pub fn needs_omz(&self) -> bool {
        !matches!(self.oh_my_zsh, OmzStatus::Installed { .. })
    }

    /// Returns true if any plugins need to be installed.
    pub fn needs_plugins(&self) -> bool {
        self.autosuggestions == PluginStatus::NotInstalled
            || self.syntax_highlighting == PluginStatus::NotInstalled
    }

    /// Returns true if any tools (fzf, bat, fd) need to be installed.
    pub fn needs_tools(&self) -> bool {
        matches!(self.fzf, FzfStatus::NotFound)
            || matches!(self.bat, BatStatus::NotFound)
            || matches!(self.fd, FdStatus::NotFound)
    }
}

/// Represents the privilege level available for package installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SudoCapability {
    /// Already running as root (no sudo needed).
    Root,
    /// Not root but sudo is available.
    SudoAvailable,
    /// No elevated privileges needed (macOS brew, Android pkg, Windows).
    NoneNeeded,
    /// Elevated privileges are needed but not available.
    NoneAvailable,
}

/// Result of configuring `~/.bashrc` auto-start.
///
/// Contains an optional warning message for cases where the existing
/// `.bashrc` content required recovery (e.g., an incomplete block was
/// removed). The caller should surface this warning to the user.
#[derive(Debug, Default)]
pub struct BashrcConfigResult {
    /// A warning message to display to the user, if any non-fatal issue was
    /// encountered and automatically recovered (e.g., a corrupt auto-start
    /// block was removed).
    pub warning: Option<String>,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_all_installed_when_everything_present() {
        let fixture = DependencyStatus {
            zsh: ZshStatus::Functional { version: "5.9".into(), path: "/usr/bin/zsh".into() },
            oh_my_zsh: OmzStatus::Installed { path: PathBuf::from("/home/user/.oh-my-zsh") },
            autosuggestions: PluginStatus::Installed,
            syntax_highlighting: PluginStatus::Installed,
            fzf: FzfStatus::Found { version: "0.54.0".into(), meets_minimum: true },
            bat: BatStatus::Installed { version: "0.24.0".into(), meets_minimum: true },
            fd: FdStatus::Installed { version: "10.2.0".into(), meets_minimum: true },
            git: true,
        };

        assert!(fixture.all_installed());
        assert!(fixture.missing_items().is_empty());
    }

    #[test]
    fn test_all_installed_false_when_zsh_missing() {
        let fixture = DependencyStatus {
            zsh: ZshStatus::NotFound,
            oh_my_zsh: OmzStatus::Installed { path: PathBuf::from("/home/user/.oh-my-zsh") },
            autosuggestions: PluginStatus::Installed,
            syntax_highlighting: PluginStatus::Installed,
            fzf: FzfStatus::NotFound,
            bat: BatStatus::NotFound,
            fd: FdStatus::NotFound,
            git: true,
        };

        assert!(!fixture.all_installed());

        let actual = fixture.missing_items();
        let expected = vec![
            ("zsh", "shell"),
            ("fzf", "fuzzy finder"),
            ("bat", "file viewer"),
            ("fd", "file finder"),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_missing_items_all_missing() {
        let fixture = DependencyStatus {
            zsh: ZshStatus::NotFound,
            oh_my_zsh: OmzStatus::NotInstalled,
            autosuggestions: PluginStatus::NotInstalled,
            syntax_highlighting: PluginStatus::NotInstalled,
            fzf: FzfStatus::NotFound,
            bat: BatStatus::NotFound,
            fd: FdStatus::NotFound,
            git: true,
        };

        let actual = fixture.missing_items();
        let expected = vec![
            ("zsh", "shell"),
            ("Oh My Zsh", "plugin framework"),
            ("zsh-autosuggestions", "plugin"),
            ("zsh-syntax-highlighting", "plugin"),
            ("fzf", "fuzzy finder"),
            ("bat", "file viewer"),
            ("fd", "file finder"),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_missing_items_partial() {
        let fixture = DependencyStatus {
            zsh: ZshStatus::Functional { version: "5.9".into(), path: "/usr/bin/zsh".into() },
            oh_my_zsh: OmzStatus::Installed { path: PathBuf::from("/home/user/.oh-my-zsh") },
            autosuggestions: PluginStatus::NotInstalled,
            syntax_highlighting: PluginStatus::Installed,
            fzf: FzfStatus::NotFound,
            bat: BatStatus::Installed { version: "0.24.0".into(), meets_minimum: true },
            fd: FdStatus::NotFound,
            git: true,
        };

        let actual = fixture.missing_items();
        let expected = vec![
            ("zsh-autosuggestions", "plugin"),
            ("fzf", "fuzzy finder"),
            ("fd", "file finder"),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_needs_zsh_when_broken() {
        let fixture = DependencyStatus {
            zsh: ZshStatus::Broken { path: "/usr/bin/zsh".into() },
            oh_my_zsh: OmzStatus::NotInstalled,
            autosuggestions: PluginStatus::NotInstalled,
            syntax_highlighting: PluginStatus::NotInstalled,
            fzf: FzfStatus::NotFound,
            bat: BatStatus::NotFound,
            fd: FdStatus::NotFound,
            git: true,
        };

        assert!(fixture.needs_zsh());
    }
}
