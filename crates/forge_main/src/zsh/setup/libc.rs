//! Libc detection for Linux systems.
//!
//! Determines whether the system uses musl or GNU libc, which affects
//! which binary variants to download for CLI tools (fzf, bat, fd).

use std::path::Path;

use anyhow::{Result, bail};
use tokio::process::Command;

use super::platform::{Platform, detect_platform};

/// Type of C standard library (libc) on Linux systems.
///
/// Used to determine which binary variant to download for CLI tools
/// (fzf, bat, fd) that provide both musl and GNU builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibcType {
    /// musl libc (statically linked, works everywhere)
    Musl,
    /// GNU libc / glibc (dynamically linked, requires compatible version)
    Gnu,
}

impl std::fmt::Display for LibcType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LibcType::Musl => write!(f, "musl"),
            LibcType::Gnu => write!(f, "GNU"),
        }
    }
}

/// Detects the libc type on Linux systems.
///
/// Uses multiple detection methods in order:
/// 1. Check for musl library files in `/lib/libc.musl-{arch}.so.1`
/// 2. Run `ldd /bin/ls` and check for "musl" in output
/// 3. Extract glibc version from `ldd --version` and verify >= 2.39
/// 4. Verify all required shared libraries exist
///
/// Returns `LibcType::Musl` as safe fallback if detection fails or
/// if glibc version is too old.
///
/// # Errors
///
/// Returns error only if running on non-Linux platform (should not be called).
pub async fn detect_libc_type() -> Result<LibcType> {
    let platform = detect_platform();
    if platform != Platform::Linux {
        bail!(
            "detect_libc_type() called on non-Linux platform: {}",
            platform
        );
    }

    // Method 1: Check for musl library files
    let arch = std::env::consts::ARCH;
    let musl_paths = [
        format!("/lib/libc.musl-{}.so.1", arch),
        format!("/usr/lib/libc.musl-{}.so.1", arch),
    ];
    for path in &musl_paths {
        if Path::new(path).exists() {
            return Ok(LibcType::Musl);
        }
    }

    // Method 2: Check ldd output for "musl"
    if let Ok(output) = Command::new("ldd").arg("/bin/ls").output().await
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.to_lowercase().contains("musl") {
            return Ok(LibcType::Musl);
        }
    }

    // Method 3: Check glibc version
    let glibc_version = extract_glibc_version().await;
    if let Some(version) = glibc_version {
        // Require glibc >= 2.39 for GNU binaries
        if version >= (2, 39) {
            // Method 4: Verify all required shared libraries exist
            if check_gnu_runtime_deps() {
                return Ok(LibcType::Gnu);
            }
        }
    }

    // Safe fallback: use musl (works everywhere)
    Ok(LibcType::Musl)
}

/// Extracts glibc version from `ldd --version` or `getconf GNU_LIBC_VERSION`.
///
/// Returns `Some((major, minor))` if version found, `None` otherwise.
async fn extract_glibc_version() -> Option<(u32, u32)> {
    // Try ldd --version first
    if let Ok(output) = Command::new("ldd").arg("--version").output().await
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(version) = parse_version_from_text(&stdout) {
            return Some(version);
        }
    }

    // Fall back to getconf
    if let Ok(output) = Command::new("getconf")
        .arg("GNU_LIBC_VERSION")
        .output()
        .await
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(version) = parse_version_from_text(&stdout) {
            return Some(version);
        }
    }

    None
}

/// Parses version string like "2.39" or "glibc 2.39" from text.
///
/// Returns `Some((major, minor))` if found, `None` otherwise.
fn parse_version_from_text(text: &str) -> Option<(u32, u32)> {
    use regex::Regex;
    let re = Regex::new(r"(\d+)\.(\d+)").ok()?;
    let caps = re.captures(text)?;
    let major = caps.get(1)?.as_str().parse().ok()?;
    let minor = caps.get(2)?.as_str().parse().ok()?;
    Some((major, minor))
}

/// Checks if all required GNU runtime dependencies are available.
///
/// Verifies existence of:
/// - `libgcc_s.so.1` (GCC runtime)
/// - `libm.so.6` (math library)
/// - `libc.so.6` (C standard library)
///
/// Returns `true` only if ALL libraries found.
fn check_gnu_runtime_deps() -> bool {
    let required_libs = ["libgcc_s.so.1", "libm.so.6", "libc.so.6"];
    let arch = std::env::consts::ARCH;
    let search_paths = [
        "/lib",
        "/lib64",
        "/usr/lib",
        "/usr/lib64",
        &format!("/lib/{}-linux-gnu", arch),
        &format!("/usr/lib/{}-linux-gnu", arch),
    ];

    for lib in &required_libs {
        let mut found = false;
        for path in &search_paths {
            let lib_path = Path::new(path).join(lib);
            if lib_path.exists() {
                found = true;
                break;
            }
        }
        if !found {
            // Fall back to ldconfig -p
            if !check_lib_with_ldconfig(lib) {
                return false;
            }
        }
    }

    true
}

/// Checks if a library exists using `ldconfig -p`.
///
/// Returns `true` if library found, `false` otherwise.
fn check_lib_with_ldconfig(lib_name: &str) -> bool {
    if let Ok(output) = std::process::Command::new("ldconfig").arg("-p").output()
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout.contains(lib_name);
    }
    false
}
