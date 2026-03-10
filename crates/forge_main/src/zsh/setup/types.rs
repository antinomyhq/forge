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

/// Reason a dependency appears in the missing list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum_macros::Display)]
pub enum ItemReason {
    /// The tool is not installed at all.
    #[strum(to_string = "missing")]
    Missing,
    /// The tool is installed but below the minimum required version.
    #[strum(to_string = "outdated")]
    Outdated,
}

/// Identifies a dependency managed by the ZSH setup orchestrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum_macros::Display)]
pub enum Dependency {
    /// zsh shell
    #[strum(to_string = "zsh")]
    Zsh,
    /// Oh My Zsh plugin framework
    #[strum(to_string = "Oh My Zsh")]
    OhMyZsh,
    /// zsh-autosuggestions plugin
    #[strum(to_string = "zsh-autosuggestions")]
    Autosuggestions,
    /// zsh-syntax-highlighting plugin
    #[strum(to_string = "zsh-syntax-highlighting")]
    SyntaxHighlighting,
    /// fzf fuzzy finder
    #[strum(to_string = "fzf")]
    Fzf,
    /// bat file viewer
    #[strum(to_string = "bat")]
    Bat,
    /// fd file finder
    #[strum(to_string = "fd")]
    Fd,
}

impl Dependency {
    /// Returns the human-readable category/kind of this dependency.
    pub fn kind(&self) -> &'static str {
        match self {
            Dependency::Zsh => "shell",
            Dependency::OhMyZsh => "plugin framework",
            Dependency::Autosuggestions | Dependency::SyntaxHighlighting => "plugin",
            Dependency::Fzf => "fuzzy finder",
            Dependency::Bat => "file viewer",
            Dependency::Fd => "file finder",
        }
    }
}

/// A dependency that needs to be installed or upgraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingItem {
    /// Which dependency is missing or outdated.
    pub dep: Dependency,
    /// Why it appears in the missing list.
    pub reason: ItemReason,
}

impl MissingItem {
    /// Creates a new missing item.
    pub fn new(dep: Dependency, reason: ItemReason) -> Self {
        Self { dep, reason }
    }

    /// Returns the human-readable category/kind of this dependency.
    pub fn kind(&self) -> &'static str {
        self.dep.kind()
    }
}

impl std::fmt::Display for MissingItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.reason {
            ItemReason::Missing => write!(f, "{}", self.dep),
            ItemReason::Outdated => write!(f, "{} ({})", self.dep, self.reason),
        }
    }
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

    /// Returns a list of dependencies that need to be installed or upgraded.
    pub fn missing_items(&self) -> Vec<MissingItem> {
        let mut items = Vec::new();
        if !matches!(self.zsh, ZshStatus::Functional { .. }) {
            items.push(MissingItem::new(Dependency::Zsh, ItemReason::Missing));
        }
        if !matches!(self.oh_my_zsh, OmzStatus::Installed { .. }) {
            items.push(MissingItem::new(Dependency::OhMyZsh, ItemReason::Missing));
        }
        if self.autosuggestions == PluginStatus::NotInstalled {
            items.push(MissingItem::new(
                Dependency::Autosuggestions,
                ItemReason::Missing,
            ));
        }
        if self.syntax_highlighting == PluginStatus::NotInstalled {
            items.push(MissingItem::new(
                Dependency::SyntaxHighlighting,
                ItemReason::Missing,
            ));
        }
        match &self.fzf {
            FzfStatus::NotFound => {
                items.push(MissingItem::new(Dependency::Fzf, ItemReason::Missing))
            }
            FzfStatus::Found { meets_minimum: false, .. } => {
                items.push(MissingItem::new(Dependency::Fzf, ItemReason::Outdated))
            }
            _ => {}
        }
        match &self.bat {
            BatStatus::NotFound => {
                items.push(MissingItem::new(Dependency::Bat, ItemReason::Missing))
            }
            BatStatus::Installed { meets_minimum: false, .. } => {
                items.push(MissingItem::new(Dependency::Bat, ItemReason::Outdated))
            }
            _ => {}
        }
        match &self.fd {
            FdStatus::NotFound => items.push(MissingItem::new(Dependency::Fd, ItemReason::Missing)),
            FdStatus::Installed { meets_minimum: false, .. } => {
                items.push(MissingItem::new(Dependency::Fd, ItemReason::Outdated))
            }
            _ => {}
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
            MissingItem::new(Dependency::Zsh, ItemReason::Missing),
            MissingItem::new(Dependency::Fzf, ItemReason::Missing),
            MissingItem::new(Dependency::Bat, ItemReason::Missing),
            MissingItem::new(Dependency::Fd, ItemReason::Missing),
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
            MissingItem::new(Dependency::Zsh, ItemReason::Missing),
            MissingItem::new(Dependency::OhMyZsh, ItemReason::Missing),
            MissingItem::new(Dependency::Autosuggestions, ItemReason::Missing),
            MissingItem::new(Dependency::SyntaxHighlighting, ItemReason::Missing),
            MissingItem::new(Dependency::Fzf, ItemReason::Missing),
            MissingItem::new(Dependency::Bat, ItemReason::Missing),
            MissingItem::new(Dependency::Fd, ItemReason::Missing),
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
            MissingItem::new(Dependency::Autosuggestions, ItemReason::Missing),
            MissingItem::new(Dependency::Fzf, ItemReason::Missing),
            MissingItem::new(Dependency::Fd, ItemReason::Missing),
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
