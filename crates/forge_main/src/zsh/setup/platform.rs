//! Platform detection for the ZSH setup orchestrator.
//!
//! Detects the current operating system platform at runtime, distinguishing
//! between Linux, macOS, Windows (Git Bash/MSYS2/Cygwin), and Android (Termux).

use std::path::Path;

/// Represents the detected operating system platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Linux (excluding Android)
    Linux,
    /// macOS / Darwin
    MacOS,
    /// Windows (Git Bash, MSYS2, Cygwin)
    Windows,
    /// Android (Termux or similar)
    Android,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Linux => write!(f, "Linux"),
            Platform::MacOS => write!(f, "macOS"),
            Platform::Windows => write!(f, "Windows"),
            Platform::Android => write!(f, "Android"),
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
}
