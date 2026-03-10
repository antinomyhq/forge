//! Platform and architecture detection for the ZSH setup orchestrator.
//!
//! Detects the current operating system platform at runtime, distinguishing
//! between Linux, macOS, Windows (Git Bash/MSYS2/Cygwin), and Android (Termux).
//! Also detects the CPU architecture for download URL construction.

use std::path::Path;

use anyhow::{Result, bail};

/// Represents the detected operating system platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum_macros::Display)]
pub enum Platform {
    /// Linux (excluding Android)
    Linux,
    /// macOS / Darwin
    #[strum(to_string = "macOS")]
    MacOS,
    /// Windows (Git Bash, MSYS2, Cygwin)
    Windows,
    /// Android (Termux or similar)
    Android,
}

impl Platform {
    /// Returns the OS identifier used in fzf release asset names.
    pub fn fzf_os(&self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::MacOS => "darwin",
            Platform::Windows => "windows",
            Platform::Android => "android",
        }
    }

    /// Returns the OS pattern used to search for matching fzf release assets.
    ///
    /// Android falls back to `"linux"` because fzf does not ship
    /// android-specific binaries.
    pub fn fzf_asset_pattern(&self) -> &'static str {
        match self {
            Platform::Android => "linux",
            other => other.fzf_os(),
        }
    }

    /// Returns the default archive extension for tool downloads on this
    /// platform.
    pub fn archive_ext(&self) -> &'static str {
        match self {
            Platform::Windows => "zip",
            _ => "tar.gz",
        }
    }
}

/// Detected CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// 64-bit x86 (Intel / AMD)
    X86_64,
    /// 64-bit ARM (Apple Silicon, Graviton, etc.)
    Aarch64,
}

impl Arch {
    /// Detects the architecture from `std::env::consts::ARCH`.
    pub fn detect() -> Result<Self> {
        match std::env::consts::ARCH {
            "x86_64" => Ok(Arch::X86_64),
            "aarch64" => Ok(Arch::Aarch64),
            other => bail!("Unsupported architecture: {}", other),
        }
    }

    /// Returns the Go-style architecture name used in fzf release URLs.
    pub fn as_go(&self) -> &'static str {
        match self {
            Arch::X86_64 => "amd64",
            Arch::Aarch64 => "arm64",
        }
    }

    /// Returns the Rust target-triple architecture prefix used in bat/fd
    /// release URLs.
    pub fn as_rust(&self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        }
    }
}

/// Detects the current operating system platform at runtime.
///
/// On Linux, further distinguishes Android from regular Linux by checking
/// for Termux environment variables and system files.
pub fn detect_platform() -> Platform {
    if cfg!(target_os = "windows") {
        return Platform::Windows;
    }
    if cfg!(target_os = "macos") {
        return Platform::MacOS;
    }
    if cfg!(target_os = "android") {
        return Platform::Android;
    }

    // On Linux, check for Android environment
    if cfg!(target_os = "linux") && is_android() {
        return Platform::Android;
    }

    // Also check the OS string at runtime for MSYS2/Cygwin environments
    let os = std::env::consts::OS;
    if os.starts_with("windows") || os.starts_with("msys") || os.starts_with("cygwin") {
        return Platform::Windows;
    }

    Platform::Linux
}

/// Checks if running on Android (Termux or similar).
fn is_android() -> bool {
    // Check Termux PREFIX
    if let Ok(prefix) = std::env::var("PREFIX")
        && prefix.contains("com.termux")
    {
        return true;
    }
    // Check Android-specific env vars
    if std::env::var("ANDROID_ROOT").is_ok() || std::env::var("ANDROID_DATA").is_ok() {
        return true;
    }
    // Check for Android build.prop
    Path::new("/system/build.prop").exists()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_detect_platform_returns_valid() {
        let actual = detect_platform();
        // On the test runner OS, we should get a valid platform
        let is_valid = matches!(
            actual,
            Platform::Linux | Platform::MacOS | Platform::Windows | Platform::Android
        );
        assert!(is_valid, "Expected valid platform, got {:?}", actual);
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(format!("{}", Platform::Linux), "Linux");
        assert_eq!(format!("{}", Platform::MacOS), "macOS");
        assert_eq!(format!("{}", Platform::Windows), "Windows");
        assert_eq!(format!("{}", Platform::Android), "Android");
    }

    #[test]
    fn test_arch_detect() {
        let actual = Arch::detect();
        assert!(actual.is_ok(), "Arch::detect() should succeed on CI");
    }
}
