//! ZSH setup orchestrator for `forge zsh setup`.
//!
//! Detects and installs all dependencies required for forge's shell
//! integration: zsh, Oh My Zsh, zsh-autosuggestions, zsh-syntax-highlighting.
//! Handles platform-specific installation (Linux, macOS, Android, Windows/Git
//! Bash) with parallel dependency detection and installation where possible.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tokio::process::Command;

// =============================================================================
// Constants
// =============================================================================

const MSYS2_BASE: &str = "https://repo.msys2.org/msys/x86_64";
const MSYS2_PKGS: &[&str] = &[
    "zsh",
    "ncurses",
    "libpcre2_8",
    "libiconv",
    "libgdbm",
    "gcc-libs",
];

const OMZ_INSTALL_URL: &str =
    "https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh";

const FZF_MIN_VERSION: &str = "0.36.0";
const BAT_MIN_VERSION: &str = "0.20.0";
const FD_MIN_VERSION: &str = "10.0.0";

// =============================================================================
// Platform Detection
// =============================================================================

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

// =============================================================================
// Libc Detection
// =============================================================================

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

// =============================================================================
// Dependency Status Types
// =============================================================================

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

// =============================================================================
// Sudo Capability
// =============================================================================

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

// =============================================================================
// Detection Functions
// =============================================================================

/// Detects whether git is available on the system.
///
/// # Returns
///
/// `true` if `git --version` succeeds, `false` otherwise.
pub async fn detect_git() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Detects the current zsh installation status.
///
/// Checks for zsh binary presence, then verifies that critical modules
/// (zle, datetime, stat) load correctly.
pub async fn detect_zsh() -> ZshStatus {
    // Find zsh binary
    let which_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };

    let output = match Command::new(which_cmd)
        .arg("zsh")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return ZshStatus::NotFound,
    };

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return ZshStatus::NotFound;
    }

    // Smoke test critical modules
    let modules_ok = Command::new("zsh")
        .args([
            "-c",
            "zmodload zsh/zle && zmodload zsh/datetime && zmodload zsh/stat",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);

    if !modules_ok {
        return ZshStatus::Broken { path: path.lines().next().unwrap_or(&path).to_string() };
    }

    // Get version
    let version = match Command::new("zsh")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(o) if o.status.success() => {
            let out = String::from_utf8_lossy(&o.stdout);
            // "zsh 5.9 (x86_64-pc-linux-gnu)" -> "5.9"
            out.split_whitespace()
                .nth(1)
                .unwrap_or("unknown")
                .to_string()
        }
        _ => "unknown".to_string(),
    };

    ZshStatus::Functional {
        version,
        path: path.lines().next().unwrap_or(&path).to_string(),
    }
}

/// Detects whether Oh My Zsh is installed.
pub async fn detect_oh_my_zsh() -> OmzStatus {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return OmzStatus::NotInstalled,
    };
    let omz_path = PathBuf::from(&home).join(".oh-my-zsh");
    if omz_path.is_dir() {
        OmzStatus::Installed { path: omz_path }
    } else {
        OmzStatus::NotInstalled
    }
}

/// Returns the `$ZSH_CUSTOM` plugins directory path.
///
/// Falls back to `$HOME/.oh-my-zsh/custom` if the environment variable is not
/// set.
fn zsh_custom_dir() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("ZSH_CUSTOM") {
        return Some(PathBuf::from(custom));
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".oh-my-zsh").join("custom"))
}

/// Detects whether the zsh-autosuggestions plugin is installed.
pub async fn detect_autosuggestions() -> PluginStatus {
    match zsh_custom_dir() {
        Some(dir) if dir.join("plugins").join("zsh-autosuggestions").is_dir() => {
            PluginStatus::Installed
        }
        _ => PluginStatus::NotInstalled,
    }
}

/// Detects whether the zsh-syntax-highlighting plugin is installed.
pub async fn detect_syntax_highlighting() -> PluginStatus {
    match zsh_custom_dir() {
        Some(dir) if dir.join("plugins").join("zsh-syntax-highlighting").is_dir() => {
            PluginStatus::Installed
        }
        _ => PluginStatus::NotInstalled,
    }
}

/// Detects fzf installation and checks version against minimum requirement.
pub async fn detect_fzf() -> FzfStatus {
    // Check if fzf exists
    if !command_exists("fzf").await {
        return FzfStatus::NotFound;
    }

    let output = match Command::new("fzf")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return FzfStatus::NotFound,
    };

    let out = String::from_utf8_lossy(&output.stdout);
    // fzf --version outputs something like "0.54.0 (d4e6f0c)" or just "0.54.0"
    let version = out
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_string();

    let meets_minimum = version_gte(&version, FZF_MIN_VERSION);

    FzfStatus::Found { version, meets_minimum }
}

/// Detects bat installation (checks both "bat" and "batcat" on Debian/Ubuntu).
pub async fn detect_bat() -> BatStatus {
    // Try "bat" first, then "batcat" (Debian/Ubuntu naming)
    for cmd in &["bat", "batcat"] {
        if command_exists(cmd).await
            && let Ok(output) = Command::new(cmd)
                .arg("--version")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
            && output.status.success()
        {
            let out = String::from_utf8_lossy(&output.stdout);
            // bat --version outputs "bat 0.24.0" or similar
            let version = out
                .split_whitespace()
                .nth(1)
                .unwrap_or("unknown")
                .to_string();
            let meets_minimum = version_gte(&version, BAT_MIN_VERSION);
            return BatStatus::Installed { version, meets_minimum };
        }
    }
    BatStatus::NotFound
}

/// Detects fd installation (checks both "fd" and "fdfind" on Debian/Ubuntu).
pub async fn detect_fd() -> FdStatus {
    // Try "fd" first, then "fdfind" (Debian/Ubuntu naming)
    for cmd in &["fd", "fdfind"] {
        if command_exists(cmd).await
            && let Ok(output) = Command::new(cmd)
                .arg("--version")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
            && output.status.success()
        {
            let out = String::from_utf8_lossy(&output.stdout);
            // fd --version outputs "fd 10.2.0" or similar
            let version = out
                .split_whitespace()
                .nth(1)
                .unwrap_or("unknown")
                .to_string();
            let meets_minimum = version_gte(&version, FD_MIN_VERSION);
            return FdStatus::Installed { version, meets_minimum };
        }
    }
    FdStatus::NotFound
}

/// Runs all dependency detection functions in parallel and returns aggregated
/// results.
///
/// # Returns
///
/// A `DependencyStatus` containing the status of all dependencies.
pub async fn detect_all_dependencies() -> DependencyStatus {
    let (git, zsh, oh_my_zsh, autosuggestions, syntax_highlighting, fzf, bat, fd) = tokio::join!(
        detect_git(),
        detect_zsh(),
        detect_oh_my_zsh(),
        detect_autosuggestions(),
        detect_syntax_highlighting(),
        detect_fzf(),
        detect_bat(),
        detect_fd(),
    );

    DependencyStatus {
        zsh,
        oh_my_zsh,
        autosuggestions,
        syntax_highlighting,
        fzf,
        bat,
        fd,
        git,
    }
}

/// Detects sudo capability for the current platform.
pub async fn detect_sudo(platform: Platform) -> SudoCapability {
    match platform {
        Platform::Windows | Platform::Android => SudoCapability::NoneNeeded,
        Platform::MacOS | Platform::Linux => {
            // Check if already root via `id -u`
            let is_root = Command::new("id")
                .arg("-u")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
                .unwrap_or(false);

            if is_root {
                return SudoCapability::Root;
            }

            // Check if sudo is available
            let has_sudo = command_exists("sudo").await;

            if has_sudo {
                SudoCapability::SudoAvailable
            } else {
                SudoCapability::NoneAvailable
            }
        }
    }
}

// =============================================================================
// Installation Functions
// =============================================================================

/// Runs a command, optionally prepending `sudo`, and returns the result.
///
/// # Arguments
///
/// * `program` - The program to run
/// * `args` - Arguments to pass
/// * `sudo` - The sudo capability level
///
/// # Errors
///
/// Returns error if:
/// - Sudo is needed but not available
/// - The command fails to spawn or exits with non-zero status
async fn run_maybe_sudo(program: &str, args: &[&str], sudo: &SudoCapability) -> Result<()> {
    let mut cmd = match sudo {
        SudoCapability::Root | SudoCapability::NoneNeeded => {
            let mut c = Command::new(program);
            c.args(args);
            c
        }
        SudoCapability::SudoAvailable => {
            let mut c = Command::new("sudo");
            c.arg(program);
            c.args(args);
            c
        }
        SudoCapability::NoneAvailable => {
            bail!("Root privileges required to install zsh. Either run as root or install sudo.");
        }
    };

    cmd.stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit());

    let status = cmd
        .status()
        .await
        .context(format!("Failed to execute {}", program))?;

    if !status.success() {
        bail!("{} exited with code {:?}", program, status.code());
    }

    Ok(())
}

/// Installs zsh using the appropriate method for the detected platform.
///
/// When `reinstall` is true, forces a reinstallation (e.g., for broken
/// modules).
///
/// # Errors
///
/// Returns error if no supported package manager is found or installation
/// fails.
pub async fn install_zsh(platform: Platform, sudo: &SudoCapability, reinstall: bool) -> Result<()> {
    match platform {
        Platform::MacOS => install_zsh_macos(sudo).await,
        Platform::Linux => install_zsh_linux(sudo, reinstall).await,
        Platform::Android => install_zsh_android().await,
        Platform::Windows => install_zsh_windows().await,
    }
}

/// Installs zsh on macOS via Homebrew.
async fn install_zsh_macos(sudo: &SudoCapability) -> Result<()> {
    if !command_exists("brew").await {
        bail!("Homebrew not found. Install from https://brew.sh then re-run forge zsh setup");
    }

    // Homebrew refuses to run as root
    if *sudo == SudoCapability::Root {
        if let Ok(brew_user) = std::env::var("SUDO_USER") {
            let status = Command::new("sudo")
                .args(["-u", &brew_user, "brew", "install", "zsh"])
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .status()
                .await
                .context("Failed to run brew as non-root user")?;

            if !status.success() {
                bail!("brew install zsh failed");
            }
            return Ok(());
        }
        bail!(
            "Homebrew cannot run as root. Please run without sudo, or install zsh manually: brew install zsh"
        );
    }

    let status = Command::new("brew")
        .args(["install", "zsh"])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to run brew install zsh")?;

    if !status.success() {
        bail!("brew install zsh failed");
    }

    Ok(())
}

/// A Linux package manager with knowledge of how to install and reinstall
/// packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum_macros::Display)]
#[strum(serialize_all = "kebab-case")]
enum LinuxPackageManager {
    /// Debian / Ubuntu family.
    AptGet,
    /// Fedora / RHEL 8+ family.
    Dnf,
    /// RHEL 7 / CentOS 7 family (legacy).
    Yum,
    /// Arch Linux family.
    Pacman,
    /// Alpine Linux.
    Apk,
    /// openSUSE family.
    Zypper,
    /// Void Linux.
    #[strum(serialize = "xbps-install")]
    XbpsInstall,
}

impl LinuxPackageManager {
    /// Returns the argument list for a standard package installation.
    fn install_args<S: AsRef<str>>(&self, packages: &[S]) -> Vec<String> {
        let mut args = match self {
            Self::AptGet => vec!["install".to_string(), "-y".to_string()],
            Self::Dnf | Self::Yum => vec!["install".to_string(), "-y".to_string()],
            Self::Pacman => vec!["-S".to_string(), "--noconfirm".to_string()],
            Self::Apk => vec!["add".to_string(), "--no-cache".to_string()],
            Self::Zypper => vec!["install".to_string(), "-y".to_string()],
            Self::XbpsInstall => vec!["-Sy".to_string()],
        };
        args.extend(packages.iter().map(|p| p.as_ref().to_string()));
        args
    }

    /// Returns the argument list that forces a full reinstall, restoring any
    /// deleted files (e.g., broken zsh module `.so` files).
    fn reinstall_args<S: AsRef<str>>(&self, packages: &[S]) -> Vec<String> {
        let mut args = match self {
            Self::AptGet => vec![
                "install".to_string(),
                "-y".to_string(),
                "--reinstall".to_string(),
            ],
            Self::Dnf | Self::Yum => vec!["reinstall".to_string(), "-y".to_string()],
            Self::Pacman => vec![
                "-S".to_string(),
                "--noconfirm".to_string(),
                "--overwrite".to_string(),
                "*".to_string(),
            ],
            Self::Apk => vec![
                "add".to_string(),
                "--no-cache".to_string(),
                "--force-overwrite".to_string(),
            ],
            Self::Zypper => vec![
                "install".to_string(),
                "-y".to_string(),
                "--force".to_string(),
            ],
            Self::XbpsInstall => vec!["-Sfy".to_string()],
        };
        args.extend(packages.iter().map(|p| p.as_ref().to_string()));
        args
    }

    /// Returns all supported package managers in detection-priority order.
    fn all() -> &'static [Self] {
        &[
            Self::AptGet,
            Self::Dnf,
            Self::Yum,
            Self::Pacman,
            Self::Apk,
            Self::Zypper,
            Self::XbpsInstall,
        ]
    }

    /// Returns the package name for fzf.
    fn fzf_package_name(&self) -> &'static str {
        "fzf"
    }

    /// Returns the package name for bat.
    ///
    /// On Debian/Ubuntu, the package is named "bat" (not "batcat").
    /// The binary is installed as "batcat" to avoid conflicts.
    fn bat_package_name(&self) -> &'static str {
        "bat"
    }

    /// Returns the package name for fd.
    ///
    /// On Debian/Ubuntu, the package is named "fd-find" due to naming
    /// conflicts.
    fn fd_package_name(&self) -> &'static str {
        match self {
            Self::AptGet => "fd-find",
            _ => "fd",
        }
    }

    /// Queries the available version of a package from the package manager.
    ///
    /// Returns None if the package is not available or version cannot be
    /// determined.
    async fn query_available_version(&self, package: &str) -> Option<String> {
        let binary = self.to_string();

        let output = match self {
            Self::AptGet => {
                // apt-cache policy <package> shows available versions
                Command::new("apt-cache")
                    .args(["policy", package])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .await
                    .ok()?
            }
            Self::Dnf | Self::Yum => {
                // dnf/yum info <package> shows available version
                Command::new(&binary)
                    .args(["info", package])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .await
                    .ok()?
            }
            Self::Pacman => {
                // pacman -Si <package> shows sync db info
                Command::new(&binary)
                    .args(["-Si", package])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .await
                    .ok()?
            }
            Self::Apk => {
                // apk info <package> shows version
                Command::new(&binary)
                    .args(["info", package])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .await
                    .ok()?
            }
            Self::Zypper => {
                // zypper info <package> shows available version
                Command::new(&binary)
                    .args(["info", package])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .await
                    .ok()?
            }
            Self::XbpsInstall => {
                // xbps-query -R <package> shows remote package info
                Command::new("xbps-query")
                    .args(["-R", package])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .await
                    .ok()?
            }
        };

        if !output.status.success() {
            return None;
        }

        let out = String::from_utf8_lossy(&output.stdout);

        // Parse version from output based on package manager
        match self {
            Self::AptGet => {
                // apt-cache policy output: "  Candidate: 0.24.0-1"
                for line in out.lines() {
                    if line.trim().starts_with("Candidate:") {
                        let version = line.split(':').nth(1)?.trim();
                        if version != "(none)" {
                            // Extract version number (strip debian revision)
                            let version = version.split('-').next()?.to_string();
                            return Some(version);
                        }
                    }
                }
            }
            Self::Dnf | Self::Yum => {
                // dnf info output: "Version     : 0.24.0"
                for line in out.lines() {
                    if line.starts_with("Version") {
                        let version = line.split(':').nth(1)?.trim().to_string();
                        return Some(version);
                    }
                }
            }
            Self::Pacman => {
                // pacman -Si output: "Version         : 0.24.0-1"
                for line in out.lines() {
                    if line.starts_with("Version") {
                        let version = line.split(':').nth(1)?.trim();
                        // Strip package revision
                        let version = version.split('-').next()?.to_string();
                        return Some(version);
                    }
                }
            }
            Self::Apk => {
                // apk info output: "bat-0.24.0-r0 description:"
                let first_line = out.lines().next()?;
                if first_line.contains(package) {
                    // Extract version between package name and description
                    let parts: Vec<&str> = first_line.split('-').collect();
                    if parts.len() >= 2 {
                        // Get version (skip package name, take version parts before -r0)
                        let version_parts: Vec<&str> = parts[1..]
                            .iter()
                            .take_while(|p| !p.starts_with('r'))
                            .copied()
                            .collect();
                        if !version_parts.is_empty() {
                            return Some(version_parts.join("-"));
                        }
                    }
                }
            }
            Self::Zypper => {
                // zypper info output: "Version: 0.24.0-1.1"
                for line in out.lines() {
                    if line.starts_with("Version") {
                        let version = line.split(':').nth(1)?.trim();
                        // Strip package revision
                        let version = version.split('-').next()?.to_string();
                        return Some(version);
                    }
                }
            }
            Self::XbpsInstall => {
                // xbps-query output: "pkgver: bat-0.24.0_1"
                for line in out.lines() {
                    if line.starts_with("pkgver:") {
                        let pkgver = line.split(':').nth(1)?.trim();
                        // Extract version (format: package-version_revision)
                        let version = pkgver.split('-').nth(1)?;
                        let version = version.split('_').next()?.to_string();
                        return Some(version);
                    }
                }
            }
        }

        None
    }
}

/// Installs zsh on Linux using the first available package manager.
///
/// When `reinstall` is true, uses reinstall flags to force re-extraction
/// of package files (e.g., when modules are broken but the package is
/// "already the newest version").
async fn install_zsh_linux(sudo: &SudoCapability, reinstall: bool) -> Result<()> {
    for mgr in LinuxPackageManager::all() {
        let binary = mgr.to_string();
        if command_exists(&binary).await {
            // apt-get requires a prior index refresh to avoid stale metadata
            if *mgr == LinuxPackageManager::AptGet {
                let _ = run_maybe_sudo(&binary, &["update", "-qq"], sudo).await;
            }
            let args = if reinstall {
                mgr.reinstall_args(&["zsh"])
            } else {
                mgr.install_args(&["zsh"])
            };
            return run_maybe_sudo(
                &binary,
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
                sudo,
            )
            .await;
        }
    }

    bail!(
        "No supported package manager found. Install zsh manually using your system's package manager."
    );
}

/// Installs zsh on Android via pkg.
async fn install_zsh_android() -> Result<()> {
    if !command_exists("pkg").await {
        bail!("pkg not found on Android. Install Termux's package manager first.");
    }

    let status = Command::new("pkg")
        .args(["install", "-y", "zsh"])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to run pkg install zsh")?;

    if !status.success() {
        bail!("pkg install zsh failed");
    }

    Ok(())
}

/// Installs zsh on Windows by downloading MSYS2 packages into Git Bash's /usr
/// tree.
///
/// Downloads zsh and its runtime dependencies (ncurses, libpcre2_8, libiconv,
/// libgdbm, gcc-libs) from the MSYS2 repository, extracts them, and copies
/// the files into the Git Bash `/usr` directory.
async fn install_zsh_windows() -> Result<()> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let temp_dir = PathBuf::from(&home).join(".forge-zsh-install-temp");

    // Clean up any previous temp directory
    if temp_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .context("Failed to create temp directory")?;

    // Ensure cleanup on exit
    let _cleanup = TempDirCleanup(temp_dir.clone());

    // Step 1: Resolve and download all packages in parallel
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("Failed to create HTTP client")?;

    let repo_index = client
        .get(format!("{}/", MSYS2_BASE))
        .send()
        .await
        .context("Failed to fetch MSYS2 repo index")?
        .text()
        .await
        .context("Failed to read MSYS2 repo index")?;

    // Download all packages in parallel
    let download_futures: Vec<_> = MSYS2_PKGS
        .iter()
        .map(|pkg| {
            let client = client.clone();
            let temp_dir = temp_dir.clone();
            let repo_index = repo_index.clone();
            async move {
                let pkg_file = resolve_msys2_package(pkg, &repo_index);
                let url = format!("{}/{}", MSYS2_BASE, pkg_file);
                let dest = temp_dir.join(format!("{}.pkg.tar.zst", pkg));

                let response = client
                    .get(&url)
                    .send()
                    .await
                    .context(format!("Failed to download {}", pkg))?;

                if !response.status().is_success() {
                    bail!("Failed to download {}: HTTP {}", pkg, response.status());
                }

                let bytes = response
                    .bytes()
                    .await
                    .context(format!("Failed to read {} response", pkg))?;

                tokio::fs::write(&dest, &bytes)
                    .await
                    .context(format!("Failed to write {}", pkg))?;

                Ok::<_, anyhow::Error>(())
            }
        })
        .collect();

    let results = futures::future::join_all(download_futures).await;
    for result in results {
        result?;
    }

    // Step 2: Detect extraction method and extract
    let extract_method = detect_extract_method(&temp_dir).await?;
    extract_all_packages(&temp_dir, &extract_method).await?;

    // Step 3: Verify zsh.exe was extracted
    if !temp_dir.join("usr").join("bin").join("zsh.exe").exists() {
        bail!("zsh.exe not found after extraction. The package may be corrupt.");
    }

    // Step 4: Copy into Git Bash /usr tree
    install_to_git_bash(&temp_dir).await?;

    // Step 5: Configure ~/.zshenv with fpath entries
    configure_zshenv().await?;

    Ok(())
}

/// Resolves the latest MSYS2 package filename for a given package name by
/// parsing the repository index HTML.
///
/// Falls back to hardcoded package names if parsing fails.
fn resolve_msys2_package(pkg_name: &str, repo_index: &str) -> String {
    // Try to find the latest package in the repo index
    let pattern = format!(
        r#"{}-[0-9][^\s"]*x86_64\.pkg\.tar\.zst"#,
        regex::escape(pkg_name)
    );
    if let Ok(re) = regex::Regex::new(&pattern) {
        let mut matches: Vec<&str> = re
            .find_iter(repo_index)
            .map(|m| m.as_str())
            // Exclude development packages
            .filter(|s| !s.contains("-devel-"))
            .collect();

        matches.sort();

        if let Some(latest) = matches.last() {
            return (*latest).to_string();
        }
    }

    // Fallback to hardcoded names
    match pkg_name {
        "zsh" => "zsh-5.9-5-x86_64.pkg.tar.zst",
        "ncurses" => "ncurses-6.6-1-x86_64.pkg.tar.zst",
        "libpcre2_8" => "libpcre2_8-10.47-1-x86_64.pkg.tar.zst",
        "libiconv" => "libiconv-1.18-2-x86_64.pkg.tar.zst",
        "libgdbm" => "libgdbm-1.26-1-x86_64.pkg.tar.zst",
        "gcc-libs" => "gcc-libs-15.2.0-1-x86_64.pkg.tar.zst",
        _ => "unknown",
    }
    .to_string()
}

/// Extraction methods available on Windows.
#[derive(Debug)]
enum ExtractMethod {
    /// zstd + tar are both available natively
    ZstdTar,
    /// 7-Zip (7z command)
    SevenZip,
    /// 7-Zip standalone (7za command)
    SevenZipA,
    /// PowerShell with a downloaded zstd.exe
    PowerShell {
        /// Path to the downloaded zstd.exe
        zstd_exe: PathBuf,
    },
}

/// Detects the best available extraction method on the system.
async fn detect_extract_method(temp_dir: &Path) -> Result<ExtractMethod> {
    // Check zstd + tar
    let has_zstd = command_exists("zstd").await;
    let has_tar = command_exists("tar").await;
    if has_zstd && has_tar {
        return Ok(ExtractMethod::ZstdTar);
    }

    // Check 7z
    if command_exists("7z").await {
        return Ok(ExtractMethod::SevenZip);
    }

    // Check 7za
    if command_exists("7za").await {
        return Ok(ExtractMethod::SevenZipA);
    }

    // Fall back to PowerShell + downloaded zstd.exe
    if command_exists("powershell.exe").await {
        let zstd_dir = temp_dir.join("zstd-tool");
        tokio::fs::create_dir_all(&zstd_dir)
            .await
            .context("Failed to create zstd tool directory")?;

        let zstd_zip_url =
            "https://github.com/facebook/zstd/releases/download/v1.5.5/zstd-v1.5.5-win64.zip";

        let client = reqwest::Client::new();
        let bytes = client
            .get(zstd_zip_url)
            .send()
            .await
            .context("Failed to download zstd")?
            .bytes()
            .await
            .context("Failed to read zstd download")?;

        let zip_path = zstd_dir.join("zstd.zip");
        tokio::fs::write(&zip_path, &bytes)
            .await
            .context("Failed to write zstd.zip")?;

        // Extract using PowerShell
        let zip_win = to_win_path(&zip_path);
        let dir_win = to_win_path(&zstd_dir);
        let ps_cmd = format!(
            "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
            zip_win, dir_win
        );

        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &ps_cmd])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .context("Failed to extract zstd.zip")?;

        if !status.success() {
            bail!("Failed to extract zstd.zip via PowerShell");
        }

        // Find zstd.exe recursively
        let zstd_exe = find_file_recursive(&zstd_dir, "zstd.exe").await;
        match zstd_exe {
            Some(path) => return Ok(ExtractMethod::PowerShell { zstd_exe: path }),
            None => bail!("Could not find zstd.exe after extraction"),
        }
    }

    bail!(
        "No extraction tool found (need zstd+tar, 7-Zip, or PowerShell). Install 7-Zip from https://www.7-zip.org/ and re-run."
    )
}

/// Extracts all downloaded MSYS2 packages in the temp directory.
async fn extract_all_packages(temp_dir: &Path, method: &ExtractMethod) -> Result<()> {
    for pkg in MSYS2_PKGS {
        let zst_file = temp_dir.join(format!("{}.pkg.tar.zst", pkg));
        let tar_file = temp_dir.join(format!("{}.pkg.tar", pkg));

        match method {
            ExtractMethod::ZstdTar => {
                run_cmd(
                    "zstd",
                    &[
                        "-d",
                        &path_str(&zst_file),
                        "-o",
                        &path_str(&tar_file),
                        "--quiet",
                    ],
                    temp_dir,
                )
                .await?;
                run_cmd("tar", &["-xf", &path_str(&tar_file)], temp_dir).await?;
                let _ = tokio::fs::remove_file(&tar_file).await;
            }
            ExtractMethod::SevenZip => {
                run_cmd("7z", &["x", "-y", &path_str(&zst_file)], temp_dir).await?;
                run_cmd("7z", &["x", "-y", &path_str(&tar_file)], temp_dir).await?;
                let _ = tokio::fs::remove_file(&tar_file).await;
            }
            ExtractMethod::SevenZipA => {
                run_cmd("7za", &["x", "-y", &path_str(&zst_file)], temp_dir).await?;
                run_cmd("7za", &["x", "-y", &path_str(&tar_file)], temp_dir).await?;
                let _ = tokio::fs::remove_file(&tar_file).await;
            }
            ExtractMethod::PowerShell { zstd_exe } => {
                let zst_win = to_win_path(&zst_file);
                let tar_win = to_win_path(&tar_file);
                let zstd_win = to_win_path(zstd_exe);
                let ps_cmd = format!("& '{}' -d '{}' -o '{}' --quiet", zstd_win, zst_win, tar_win);
                let status = Command::new("powershell.exe")
                    .args(["-NoProfile", "-Command", &ps_cmd])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await
                    .context(format!("Failed to decompress {}", pkg))?;

                if !status.success() {
                    bail!("Failed to decompress {}", pkg);
                }

                run_cmd("tar", &["-xf", &path_str(&tar_file)], temp_dir).await?;
                let _ = tokio::fs::remove_file(&tar_file).await;
            }
        }
    }

    Ok(())
}

/// Copies extracted zsh files into Git Bash's /usr tree.
///
/// Attempts UAC elevation via PowerShell if needed.
async fn install_to_git_bash(temp_dir: &Path) -> Result<()> {
    let git_usr = if command_exists("cygpath").await {
        let output = Command::new("cygpath")
            .args(["-w", "/usr"])
            .stdout(std::process::Stdio::piped())
            .output()
            .await?;
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        r"C:\Program Files\Git\usr".to_string()
    };

    let temp_win = to_win_path(temp_dir);

    // Generate PowerShell install script
    let ps_script = format!(
        r#"$src = '{}'
$usr = '{}'
Get-ChildItem -Path "$src\usr\bin" -Filter "*.exe" | ForEach-Object {{
    Copy-Item -Force $_.FullName "$usr\bin\"
}}
Get-ChildItem -Path "$src\usr\bin" -Filter "*.dll" | ForEach-Object {{
    Copy-Item -Force $_.FullName "$usr\bin\"
}}
if (Test-Path "$src\usr\lib\zsh") {{
    Copy-Item -Recurse -Force "$src\usr\lib\zsh" "$usr\lib\"
}}
if (Test-Path "$src\usr\share\zsh") {{
    Copy-Item -Recurse -Force "$src\usr\share\zsh" "$usr\share\"
}}
Write-Host "ZSH_INSTALL_OK""#,
        temp_win, git_usr
    );

    let ps_file = temp_dir.join("install.ps1");
    tokio::fs::write(&ps_file, &ps_script)
        .await
        .context("Failed to write install script")?;

    let ps_file_win = to_win_path(&ps_file);

    // Try elevated install via UAC
    let uac_cmd = format!(
        "Start-Process powershell -Verb RunAs -Wait -ArgumentList \"-NoProfile -ExecutionPolicy Bypass -File `\"{}`\"\"",
        ps_file_win
    );

    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &uac_cmd])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    // Fallback: direct execution if already admin
    if !Path::new("/usr/bin/zsh.exe").exists() {
        let _ = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &ps_file_win,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
    }

    if !Path::new("/usr/bin/zsh.exe").exists() {
        bail!(
            "zsh.exe not found in /usr/bin after installation. Try re-running from an Administrator Git Bash."
        );
    }

    Ok(())
}

/// Configures `~/.zshenv` with fpath entries for MSYS2 zsh function
/// subdirectories.
async fn configure_zshenv() -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let zshenv_path = PathBuf::from(&home).join(".zshenv");

    let mut content = if zshenv_path.exists() {
        tokio::fs::read_to_string(&zshenv_path)
            .await
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Remove any previous installer block
    if let (Some(start), Some(end)) = (
        content.find("# --- zsh installer fpath"),
        content.find("# --- end zsh installer fpath ---"),
    ) && start < end
    {
        let end_of_line = content[end..]
            .find('\n')
            .map(|i| end + i + 1)
            .unwrap_or(content.len());
        content.replace_range(start..end_of_line, "");
    }

    let fpath_block = r#"
# --- zsh installer fpath (added by forge zsh setup) ---
_zsh_fn_base="/usr/share/zsh/functions"
if [ -d "$_zsh_fn_base" ]; then
  fpath=("$_zsh_fn_base" $fpath)
  for _zsh_fn_sub in "$_zsh_fn_base"/*/; do
    [ -d "$_zsh_fn_sub" ] && fpath=("${_zsh_fn_sub%/}" $fpath)
  done
fi
unset _zsh_fn_base _zsh_fn_sub
# --- end zsh installer fpath ---
"#;

    content.push_str(fpath_block);
    tokio::fs::write(&zshenv_path, &content)
        .await
        .context("Failed to write ~/.zshenv")?;

    Ok(())
}

/// Installs Oh My Zsh by downloading and executing the official install script.
///
/// Sets `RUNZSH=no` and `CHSH=no` to prevent the script from switching shells
/// or starting zsh automatically (we handle that ourselves).
///
/// # Errors
///
/// Returns error if the download fails or the install script exits with
/// non-zero.
pub async fn install_oh_my_zsh() -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Failed to create HTTP client")?;

    let script = client
        .get(OMZ_INSTALL_URL)
        .send()
        .await
        .context("Failed to download Oh My Zsh install script")?
        .text()
        .await
        .context("Failed to read Oh My Zsh install script")?;

    // Write to temp file
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join("omz-install.sh");
    tokio::fs::write(&script_path, &script)
        .await
        .context("Failed to write Oh My Zsh install script")?;

    // Execute the script with RUNZSH=no and CHSH=no to prevent auto-start
    // and shell changing - we handle those ourselves
    let status = Command::new("sh")
        .arg(&script_path)
        .env("RUNZSH", "no")
        .env("CHSH", "no")
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to execute Oh My Zsh install script")?;

    // Clean up temp script
    let _ = tokio::fs::remove_file(&script_path).await;

    if !status.success() {
        bail!("Oh My Zsh installation failed. Install manually: https://ohmyz.sh/#install");
    }

    // Configure Oh My Zsh defaults in .zshrc
    configure_omz_defaults().await?;

    Ok(())
}

/// Configures Oh My Zsh defaults in `.zshrc` (theme and plugins).
async fn configure_omz_defaults() -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let zshrc_path = PathBuf::from(&home).join(".zshrc");

    if !zshrc_path.exists() {
        return Ok(());
    }

    let content = tokio::fs::read_to_string(&zshrc_path)
        .await
        .context("Failed to read .zshrc")?;

    // Create backup before modifying
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let backup_path = zshrc_path.with_file_name(format!(".zshrc.bak.{}", timestamp));
    tokio::fs::copy(&zshrc_path, &backup_path)
        .await
        .context("Failed to create .zshrc backup")?;

    let mut new_content = content.clone();

    // Set theme to robbyrussell
    let theme_re = regex::Regex::new(r#"(?m)^ZSH_THEME=.*$"#).unwrap();
    new_content = theme_re
        .replace(&new_content, r#"ZSH_THEME="robbyrussell""#)
        .to_string();

    // Set plugins
    let plugins_re = regex::Regex::new(r#"(?m)^plugins=\(.*\)$"#).unwrap();
    new_content = plugins_re
        .replace(
            &new_content,
            "plugins=(git command-not-found colored-man-pages extract z)",
        )
        .to_string();

    tokio::fs::write(&zshrc_path, &new_content)
        .await
        .context("Failed to write .zshrc")?;

    Ok(())
}

/// Installs the zsh-autosuggestions plugin via git clone into the Oh My Zsh
/// custom plugins directory.
///
/// # Errors
///
/// Returns error if git clone fails.
pub async fn install_autosuggestions() -> Result<()> {
    let dest = zsh_custom_dir()
        .context("Could not determine ZSH_CUSTOM directory")?
        .join("plugins")
        .join("zsh-autosuggestions");

    if dest.exists() {
        return Ok(());
    }

    let status = Command::new("git")
        .args([
            "clone",
            "https://github.com/zsh-users/zsh-autosuggestions.git",
            &path_str(&dest),
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to clone zsh-autosuggestions")?;

    if !status.success() {
        bail!("Failed to install zsh-autosuggestions");
    }

    Ok(())
}

/// Installs the zsh-syntax-highlighting plugin via git clone into the Oh My Zsh
/// custom plugins directory.
///
/// # Errors
///
/// Returns error if git clone fails.
pub async fn install_syntax_highlighting() -> Result<()> {
    let dest = zsh_custom_dir()
        .context("Could not determine ZSH_CUSTOM directory")?
        .join("plugins")
        .join("zsh-syntax-highlighting");

    if dest.exists() {
        return Ok(());
    }

    let status = Command::new("git")
        .args([
            "clone",
            "https://github.com/zsh-users/zsh-syntax-highlighting.git",
            &path_str(&dest),
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to clone zsh-syntax-highlighting")?;

    if !status.success() {
        bail!("Failed to install zsh-syntax-highlighting");
    }

    Ok(())
}

/// Configures `~/.bashrc` to auto-start zsh on Windows (Git Bash).
///
/// Creates necessary startup files if they don't exist, removes any previous
/// auto-start block, and appends a new one.
///
/// # Errors
///
/// Returns error if HOME is not set or file operations fail.
pub async fn configure_bashrc_autostart() -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let home_path = PathBuf::from(&home);

    // Create empty files to suppress Git Bash warnings
    for file in &[".bash_profile", ".bash_login", ".profile"] {
        let path = home_path.join(file);
        if !path.exists() {
            let _ = tokio::fs::write(&path, "").await;
        }
    }

    let bashrc_path = home_path.join(".bashrc");

    // Read or create .bashrc
    let mut content = if bashrc_path.exists() {
        tokio::fs::read_to_string(&bashrc_path)
            .await
            .unwrap_or_default()
    } else {
        "# Created by forge zsh setup\n".to_string()
    };

    // Remove any previous auto-start blocks (from old installer or from us)
    for marker in &["# Added by zsh installer", "# Added by forge zsh setup"] {
        if let Some(start) = content.find(marker) {
            // Find the closing "fi" line
            if let Some(fi_offset) = content[start..].find("\nfi\n") {
                let end = start + fi_offset + 4; // +4 for "\nfi\n"
                content.replace_range(start..end, "");
            } else if let Some(fi_offset) = content[start..].find("\nfi") {
                let end = start + fi_offset + 3;
                content.replace_range(start..end, "");
            }
        }
    }

    // Resolve zsh path
    let zsh_path = resolve_zsh_path().await;

    let autostart_block = format!(
        r#"
# Added by forge zsh setup
if [ -t 0 ] && [ -x "{zsh}" ]; then
  export SHELL="{zsh}"
  exec "{zsh}"
fi
"#,
        zsh = zsh_path
    );

    content.push_str(&autostart_block);

    tokio::fs::write(&bashrc_path, &content)
        .await
        .context("Failed to write ~/.bashrc")?;

    Ok(())
}

// =============================================================================
// Tool Installation (fzf, bat, fd)
// =============================================================================

/// Installs fzf (fuzzy finder) using package manager or GitHub releases.
///
/// Tries package manager first for faster installation and system integration.
/// Falls back to downloading from GitHub releases if package manager
/// unavailable.
///
/// # Errors
///
/// Installs fzf (fuzzy finder) using package manager or GitHub releases.
///
/// Tries package manager first (which checks version requirements before
/// installing). Falls back to GitHub releases if package manager unavailable or
/// version too old.
pub async fn install_fzf(platform: Platform, sudo: &SudoCapability) -> Result<()> {
    // Try package manager first (version is checked before installing)
    let pkg_mgr_result = match platform {
        Platform::Linux => install_via_package_manager_linux("fzf", sudo).await,
        Platform::MacOS => {
            if command_exists("brew").await {
                let status = Command::new("brew")
                    .args(["install", "fzf"])
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .await?;
                if status.success() {
                    Ok(())
                } else {
                    bail!("brew install fzf failed")
                }
            } else {
                bail!("brew not found")
            }
        }
        Platform::Android => {
            if command_exists("pkg").await {
                let status = Command::new("pkg")
                    .args(["install", "-y", "fzf"])
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .await?;
                if status.success() {
                    Ok(())
                } else {
                    bail!("pkg install fzf failed")
                }
            } else {
                bail!("pkg not found")
            }
        }
        Platform::Windows => {
            bail!("No package manager on Windows")
        }
    };

    // If package manager succeeded, verify installation and version
    if pkg_mgr_result.is_ok() {
        // Verify the tool was installed with correct version
        let status = detect_fzf().await;
        if matches!(status, FzfStatus::Found { meets_minimum: true, .. }) {
            return Ok(());
        }
        // Package manager installed old version or tool not found, fall back to GitHub
        match status {
            FzfStatus::Found { version, meets_minimum: false } => {
                eprintln!(
                    "Package manager installed fzf {}, but {} or higher required. Installing from GitHub...",
                    version, FZF_MIN_VERSION
                );
            }
            FzfStatus::NotFound => {
                eprintln!(
                    "fzf not detected after package manager installation. Installing from GitHub..."
                );
            }
            FzfStatus::Found { meets_minimum: true, .. } => {
                // Already handled above, this branch is unreachable
                unreachable!("fzf with correct version should have returned early");
            }
        }
    }

    // Fall back to GitHub releases (pkg mgr unavailable or version too old)
    install_fzf_from_github(platform).await
}
/// Installs bat (file viewer) using package manager or GitHub releases.
///
/// Tries package manager first (which checks version requirements before
/// installing). Falls back to GitHub releases if package manager unavailable or
/// version too old.
pub async fn install_bat(platform: Platform, sudo: &SudoCapability) -> Result<()> {
    // Try package manager first (version is checked before installing)
    let pkg_mgr_result = match platform {
        Platform::Linux => install_via_package_manager_linux("bat", sudo).await,
        Platform::MacOS => {
            if command_exists("brew").await {
                let status = Command::new("brew")
                    .args(["install", "bat"])
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .await?;
                if status.success() {
                    Ok(())
                } else {
                    bail!("brew install bat failed")
                }
            } else {
                bail!("brew not found")
            }
        }
        Platform::Android => {
            if command_exists("pkg").await {
                let status = Command::new("pkg")
                    .args(["install", "-y", "bat"])
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .await?;
                if status.success() {
                    Ok(())
                } else {
                    bail!("pkg install bat failed")
                }
            } else {
                bail!("pkg not found")
            }
        }
        Platform::Windows => {
            bail!("No package manager on Windows")
        }
    };

    // If package manager succeeded, verify installation and version
    if pkg_mgr_result.is_ok() {
        // Verify the tool was installed with correct version
        let status = detect_bat().await;
        if matches!(status, BatStatus::Installed { meets_minimum: true, .. }) {
            return Ok(());
        }
        // Package manager installed old version or tool not found, fall back to GitHub
        match status {
            BatStatus::Installed { version, meets_minimum: false } => {
                eprintln!(
                    "Package manager installed bat {}, but {} or higher required. Installing from GitHub...",
                    version, BAT_MIN_VERSION
                );
            }
            BatStatus::NotFound => {
                eprintln!(
                    "bat not detected after package manager installation. Installing from GitHub..."
                );
            }
            BatStatus::Installed { meets_minimum: true, .. } => {
                // Already handled above, this branch is unreachable
                unreachable!("bat with correct version should have returned early");
            }
        }
    }

    // Fall back to GitHub releases (pkg mgr unavailable or version too old)
    install_bat_from_github(platform).await
}

/// Installs fd (file finder) using package manager or GitHub releases.
///
/// Tries package manager first (which checks version requirements before
/// installing). Falls back to GitHub releases if package manager unavailable or
/// version too old.
pub async fn install_fd(platform: Platform, sudo: &SudoCapability) -> Result<()> {
    // Try package manager first (version is checked before installing)
    let pkg_mgr_result = match platform {
        Platform::Linux => install_via_package_manager_linux("fd", sudo).await,
        Platform::MacOS => {
            if command_exists("brew").await {
                let status = Command::new("brew")
                    .args(["install", "fd"])
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .await?;
                if status.success() {
                    Ok(())
                } else {
                    bail!("brew install fd failed")
                }
            } else {
                bail!("brew not found")
            }
        }
        Platform::Android => {
            if command_exists("pkg").await {
                let status = Command::new("pkg")
                    .args(["install", "-y", "fd"])
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .await?;
                if status.success() {
                    Ok(())
                } else {
                    bail!("pkg install fd failed")
                }
            } else {
                bail!("pkg not found")
            }
        }
        Platform::Windows => {
            bail!("No package manager on Windows")
        }
    };

    // If package manager succeeded, verify installation and version
    if pkg_mgr_result.is_ok() {
        // Verify the tool was installed with correct version
        let status = detect_fd().await;
        if matches!(status, FdStatus::Installed { meets_minimum: true, .. }) {
            return Ok(());
        }
        // Package manager installed old version or tool not found, fall back to GitHub
        match status {
            FdStatus::Installed { version, meets_minimum: false } => {
                eprintln!(
                    "Package manager installed fd {}, but {} or higher required. Installing from GitHub...",
                    version, FD_MIN_VERSION
                );
            }
            FdStatus::NotFound => {
                eprintln!(
                    "fd not detected after package manager installation. Installing from GitHub..."
                );
            }
            _ => {}
        }
    }

    // Fall back to GitHub releases (pkg mgr unavailable or version too old)
    install_fd_from_github(platform).await
}

/// Installs a tool via Linux package manager.
///
/// Detects available package manager, checks if available version meets minimum
/// requirements, and only installs if version is sufficient. Returns error if
/// package manager version is too old (caller should fall back to GitHub).
async fn install_via_package_manager_linux(tool: &str, sudo: &SudoCapability) -> Result<()> {
    for mgr in LinuxPackageManager::all() {
        let binary = mgr.to_string();
        if command_exists(&binary).await {
            // apt-get requires index refresh
            if *mgr == LinuxPackageManager::AptGet {
                let _ = run_maybe_sudo(&binary, &["update", "-qq"], sudo).await;
            }

            let package_name = match tool {
                "fzf" => mgr.fzf_package_name(),
                "bat" => mgr.bat_package_name(),
                "fd" => mgr.fd_package_name(),
                _ => bail!("Unknown tool: {}", tool),
            };

            // Check available version before installing
            let min_version = match tool {
                "fzf" => FZF_MIN_VERSION,
                "bat" => BAT_MIN_VERSION,
                "fd" => FD_MIN_VERSION,
                _ => bail!("Unknown tool: {}", tool),
            };

            if let Some(available_version) = mgr.query_available_version(package_name).await {
                if !version_gte(&available_version, min_version) {
                    bail!(
                        "Package manager has {} {} but {} or higher required",
                        tool,
                        available_version,
                        min_version
                    );
                }
                // Version is good, proceed with installation
            } else {
                // Could not determine version, try installing anyway
                eprintln!(
                    "Warning: Could not determine available version for {}, attempting installation anyway",
                    tool
                );
            }

            let args = mgr.install_args(&[package_name]);
            return run_maybe_sudo(
                &binary,
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
                sudo,
            )
            .await;
        }
    }
    bail!("No supported package manager found")
}

/// Installs fzf from GitHub releases.
async fn install_fzf_from_github(platform: Platform) -> Result<()> {
    // Determine the asset pattern based on platform
    let asset_pattern = match platform {
        Platform::Linux => "linux",
        Platform::MacOS => "darwin",
        Platform::Windows => "windows",
        Platform::Android => "linux", // fzf doesn't have android-specific builds
    };

    let version = get_latest_release_with_binary("junegunn/fzf", asset_pattern, "0.56.3").await;

    let url = construct_fzf_url(&version, platform)?;
    let archive_type = if platform == Platform::Windows {
        ArchiveType::Zip
    } else {
        ArchiveType::TarGz
    };

    let binary_path = download_and_extract_tool(&url, "fzf", archive_type, false).await?;
    install_binary_to_local_bin(&binary_path, "fzf").await?;

    Ok(())
}

/// Installs bat from GitHub releases.
async fn install_bat_from_github(platform: Platform) -> Result<()> {
    let target = construct_rust_target(platform).await?;

    // Find the latest release that has this specific binary
    let version = get_latest_release_with_binary("sharkdp/bat", &target, "0.24.0").await;
    let url = format!(
        "https://github.com/sharkdp/bat/releases/download/v{}/bat-v{}-{}.tar.gz",
        version, version, target
    );

    let archive_type = if platform == Platform::Windows {
        ArchiveType::Zip
    } else {
        ArchiveType::TarGz
    };

    let binary_path = download_and_extract_tool(&url, "bat", archive_type, true).await?;
    install_binary_to_local_bin(&binary_path, "bat").await?;

    Ok(())
}

/// Installs fd from GitHub releases.
async fn install_fd_from_github(platform: Platform) -> Result<()> {
    let target = construct_rust_target(platform).await?;

    // Find the latest release that has this specific binary
    let version = get_latest_release_with_binary("sharkdp/fd", &target, "10.1.0").await;
    let url = format!(
        "https://github.com/sharkdp/fd/releases/download/v{}/fd-v{}-{}.tar.gz",
        version, version, target
    );

    let archive_type = if platform == Platform::Windows {
        ArchiveType::Zip
    } else {
        ArchiveType::TarGz
    };

    let binary_path = download_and_extract_tool(&url, "fd", archive_type, true).await?;
    install_binary_to_local_bin(&binary_path, "fd").await?;

    Ok(())
}

/// Minimal struct for parsing GitHub release API response
#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

/// Minimal struct for parsing GitHub asset info
#[derive(serde::Deserialize)]
struct GitHubAsset {
    name: String,
}

/// Finds the latest GitHub release that has the required binary asset.
///
/// Checks recent releases (up to 10) and returns the first one that has
/// a binary matching the pattern. This handles cases where the latest release
/// exists but binaries haven't been built yet (CI delays).
///
/// # Arguments
/// * `repo` - Repository in format "owner/name"
/// * `asset_pattern` - Pattern to match in asset names (e.g.,
///   "x86_64-unknown-linux-musl")
///
/// Returns the version string (without 'v' prefix) or fallback if all fail.
async fn get_latest_release_with_binary(repo: &str, asset_pattern: &str, fallback: &str) -> String {
    // Try to get list of recent releases
    let releases_url = format!("https://api.github.com/repos/{}/releases?per_page=10", repo);
    let response = match reqwest::Client::new()
        .get(&releases_url)
        .header("User-Agent", "forge-cli")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp,
        _ => return fallback.to_string(),
    };

    // Parse releases
    let releases: Vec<GitHubRelease> = match response.json().await {
        Ok(r) => r,
        Err(_) => return fallback.to_string(),
    };

    // Find the first release that has the required binary
    for release in releases {
        // Check if this release has a binary matching our pattern
        let has_binary = release
            .assets
            .iter()
            .any(|asset| asset.name.contains(asset_pattern));

        if has_binary {
            // Strip 'v' prefix if present
            let version = release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name)
                .to_string();
            return version;
        }
    }

    // No release with binaries found, use fallback
    fallback.to_string()
}

/// Gets the latest release version from a GitHub repository.
///
/// Uses redirect method first (no API quota), falls back to API if needed.
/// Returns `None` if both methods fail (rate limit, offline, etc.).
async fn get_latest_github_release(repo: &str) -> Option<String> {
    // Method 1: Follow redirect from /releases/latest
    let redirect_url = format!("https://github.com/{}/releases/latest", repo);
    if let Ok(response) = reqwest::Client::new().get(&redirect_url).send().await
        && let Some(mut final_url) = response.url().path_segments()
        && let Some(tag) = final_url.next_back()
    {
        let version = tag.trim_start_matches('v').to_string();
        if !version.is_empty() {
            return Some(version);
        }
    }

    // Method 2: GitHub API (has rate limits)
    let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    if let Ok(response) = reqwest::get(&api_url).await
        && let Ok(json) = response.json::<serde_json::Value>().await
        && let Some(tag_name) = json.get("tag_name").and_then(|v| v.as_str())
    {
        let version = tag_name.trim_start_matches('v').to_string();
        return Some(version);
    }

    None
}

/// Archive type for tool downloads.
#[derive(Debug, Clone, Copy)]
enum ArchiveType {
    TarGz,
    Zip,
}

/// Downloads and extracts a tool from a URL.
///
/// Returns the path to the extracted binary.
async fn download_and_extract_tool(
    url: &str,
    tool_name: &str,
    archive_type: ArchiveType,
    nested: bool,
) -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join(format!("forge-{}-download", tool_name));
    tokio::fs::create_dir_all(&temp_dir).await?;

    // Download archive
    let response = reqwest::get(url).await.context("Failed to download tool")?;

    // Check if download was successful
    if !response.status().is_success() {
        bail!(
            "Failed to download {}: HTTP {} - {}",
            tool_name,
            response.status(),
            response.text().await.unwrap_or_default()
        );
    }

    let bytes = response.bytes().await?;

    let archive_ext = match archive_type {
        ArchiveType::TarGz => "tar.gz",
        ArchiveType::Zip => "zip",
    };
    let archive_path = temp_dir.join(format!("{}.{}", tool_name, archive_ext));
    tokio::fs::write(&archive_path, &bytes).await?;

    // Extract archive
    match archive_type {
        ArchiveType::TarGz => {
            let status = Command::new("tar")
                .args(["-xzf", &path_str(&archive_path), "-C", &path_str(&temp_dir)])
                .status()
                .await?;
            if !status.success() {
                bail!("Failed to extract tar.gz archive");
            }
        }
        ArchiveType::Zip => {
            #[cfg(target_os = "windows")]
            {
                let status = Command::new("powershell")
                    .args([
                        "-Command",
                        &format!(
                            "Expand-Archive -Path '{}' -DestinationPath '{}'",
                            archive_path.display(),
                            temp_dir.display()
                        ),
                    ])
                    .status()
                    .await?;
                if !status.success() {
                    bail!("Failed to extract zip archive");
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                let status = Command::new("unzip")
                    .args(["-q", &path_str(&archive_path), "-d", &path_str(&temp_dir)])
                    .status()
                    .await?;
                if !status.success() {
                    bail!("Failed to extract zip archive");
                }
            }
        }
    }

    // Find binary in extracted files
    let binary_name = if cfg!(target_os = "windows") {
        format!("{}.exe", tool_name)
    } else {
        tool_name.to_string()
    };

    let binary_path = if nested {
        // Nested structure: look in subdirectories
        let mut entries = tokio::fs::read_dir(&temp_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let candidate = entry.path().join(&binary_name);
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }
        bail!("Binary not found in nested archive structure");
    } else {
        // Flat structure: binary at top level
        let candidate = temp_dir.join(&binary_name);
        if candidate.exists() {
            candidate
        } else {
            bail!("Binary not found in flat archive structure");
        }
    };

    Ok(binary_path)
}

/// Installs a binary to `~/.local/bin`.
async fn install_binary_to_local_bin(binary_path: &Path, name: &str) -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let local_bin = PathBuf::from(home).join(".local").join("bin");
    tokio::fs::create_dir_all(&local_bin).await?;

    let dest = local_bin.join(name);
    tokio::fs::copy(binary_path, &dest).await?;

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&dest).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&dest, perms).await?;
    }

    Ok(())
}

/// Constructs the download URL for fzf based on platform and architecture.
fn construct_fzf_url(version: &str, platform: Platform) -> Result<String> {
    let arch = std::env::consts::ARCH;
    let (os, arch_suffix, ext) = match platform {
        Platform::Linux => {
            let arch_name = match arch {
                "x86_64" => "amd64",
                "aarch64" => "arm64",
                _ => bail!("Unsupported architecture: {}", arch),
            };
            ("linux", arch_name, "tar.gz")
        }
        Platform::MacOS => {
            let arch_name = match arch {
                "x86_64" => "amd64",
                "aarch64" => "arm64",
                _ => bail!("Unsupported architecture: {}", arch),
            };
            ("darwin", arch_name, "tar.gz")
        }
        Platform::Windows => {
            let arch_name = match arch {
                "x86_64" => "amd64",
                "aarch64" => "arm64",
                _ => bail!("Unsupported architecture: {}", arch),
            };
            ("windows", arch_name, "zip")
        }
        Platform::Android => ("android", "arm64", "tar.gz"),
    };

    Ok(format!(
        "https://github.com/junegunn/fzf/releases/download/v{}/fzf-{}-{}_{}.{}",
        version, version, os, arch_suffix, ext
    ))
}

/// Constructs a Rust target triple for bat/fd downloads.
async fn construct_rust_target(platform: Platform) -> Result<String> {
    let arch = std::env::consts::ARCH;
    match platform {
        Platform::Linux => {
            let libc = detect_libc_type().await.unwrap_or(LibcType::Musl);
            let arch_prefix = match arch {
                "x86_64" => "x86_64",
                "aarch64" => "aarch64",
                _ => bail!("Unsupported architecture: {}", arch),
            };
            let libc_suffix = match libc {
                LibcType::Musl => "musl",
                LibcType::Gnu => "gnu",
            };
            Ok(format!("{}-unknown-linux-{}", arch_prefix, libc_suffix))
        }
        Platform::MacOS => {
            let arch_prefix = match arch {
                "x86_64" => "x86_64",
                "aarch64" => "aarch64",
                _ => bail!("Unsupported architecture: {}", arch),
            };
            Ok(format!("{}-apple-darwin", arch_prefix))
        }
        Platform::Windows => Ok("x86_64-pc-windows-msvc".to_string()),
        Platform::Android => Ok("aarch64-unknown-linux-musl".to_string()),
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Checks if a command exists on the system using POSIX-compliant
/// `command -v` (available on all Unix shells) or `where` on Windows.
async fn command_exists(cmd: &str) -> bool {
    if cfg!(target_os = "windows") {
        Command::new("where")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        // Use `sh -c "command -v <cmd>"` which is POSIX-compliant and
        // available on all systems, unlike `which` which is an external
        // utility not present on minimal containers (Arch, Fedora, etc.)
        Command::new("sh")
            .args(["-c", &format!("command -v {cmd}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Runs a command in a given working directory, inheriting stdout/stderr.
async fn run_cmd(program: &str, args: &[&str], cwd: &Path) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .context(format!("Failed to run {}", program))?;

    if !status.success() {
        bail!("{} failed with exit code {:?}", program, status.code());
    }
    Ok(())
}

/// Converts a path to a string, using lossy conversion.
fn path_str(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

/// Converts a Unix-style path to a Windows path.
///
/// Uses `cygpath` if available, otherwise performs manual `/c/...` -> `C:\...`
/// conversion.
fn to_win_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    // Simple conversion: /c/Users/... -> C:\Users\...
    if s.len() >= 3 && s.starts_with('/') && s.chars().nth(2) == Some('/') {
        let drive = s.chars().nth(1).unwrap().to_uppercase().to_string();
        let rest = &s[2..];
        format!("{}:{}", drive, rest.replace('/', "\\"))
    } else {
        s.replace('/', "\\")
    }
}

/// Recursively searches for a file by name in a directory.
async fn find_file_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return None,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == name).unwrap_or(false) {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = Box::pin(find_file_recursive(&path, name)).await
        {
            return Some(found);
        }
    }

    None
}

/// Resolves the path to the zsh binary.
async fn resolve_zsh_path() -> String {
    let which = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    match Command::new(which)
        .arg("zsh")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(o) if o.status.success() => {
            let out = String::from_utf8_lossy(&o.stdout);
            out.lines().next().unwrap_or("zsh").trim().to_string()
        }
        _ => "zsh".to_string(),
    }
}

/// Compares two version strings (dotted numeric).
///
/// Returns `true` if `version >= minimum`.
fn version_gte(version: &str, minimum: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .map(|p| {
                // Remove non-numeric suffixes like "0-rc1"
                let numeric: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                numeric.parse().unwrap_or(0)
            })
            .collect()
    };

    let ver = parse(version);
    let min = parse(minimum);

    for i in 0..std::cmp::max(ver.len(), min.len()) {
        let v = ver.get(i).copied().unwrap_or(0);
        let m = min.get(i).copied().unwrap_or(0);
        if v > m {
            return true;
        }
        if v < m {
            return false;
        }
    }
    true // versions are equal
}

/// RAII guard that cleans up a temporary directory on drop.
struct TempDirCleanup(PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        // Best effort cleanup — don't block on async in drop
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    // ---- Version comparison tests ----

    #[test]
    fn test_version_gte_equal() {
        assert!(version_gte("0.36.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_greater_major() {
        assert!(version_gte("1.0.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_greater_minor() {
        assert!(version_gte("0.54.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_less() {
        assert!(!version_gte("0.35.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_with_v_prefix() {
        assert!(version_gte("v0.54.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_with_rc_suffix() {
        assert!(version_gte("0.54.0-rc1", "0.36.0"));
    }

    // ---- Platform detection tests ----

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

    // ---- DependencyStatus tests ----

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

    // ---- MSYS2 package resolution tests ----

    #[test]
    fn test_resolve_msys2_package_fallback() {
        // Empty repo index should fall back to hardcoded names
        let actual = resolve_msys2_package("zsh", "");
        let expected = "zsh-5.9-5-x86_64.pkg.tar.zst";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_msys2_package_from_index() {
        let fake_index = r#"
            <a href="zsh-5.9-3-x86_64.pkg.tar.zst">zsh-5.9-3-x86_64.pkg.tar.zst</a>
            <a href="zsh-5.9-5-x86_64.pkg.tar.zst">zsh-5.9-5-x86_64.pkg.tar.zst</a>
            <a href="zsh-5.8-1-x86_64.pkg.tar.zst">zsh-5.8-1-x86_64.pkg.tar.zst</a>
        "#;
        let actual = resolve_msys2_package("zsh", fake_index);
        let expected = "zsh-5.9-5-x86_64.pkg.tar.zst";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_msys2_package_excludes_devel() {
        let fake_index = r#"
            <a href="ncurses-devel-6.6-1-x86_64.pkg.tar.zst">ncurses-devel-6.6-1-x86_64.pkg.tar.zst</a>
            <a href="ncurses-6.6-1-x86_64.pkg.tar.zst">ncurses-6.6-1-x86_64.pkg.tar.zst</a>
        "#;
        let actual = resolve_msys2_package("ncurses", fake_index);
        let expected = "ncurses-6.6-1-x86_64.pkg.tar.zst";
        assert_eq!(actual, expected);
    }

    // ---- Windows path conversion tests ----

    #[test]
    fn test_to_win_path_drive() {
        let actual = to_win_path(Path::new("/c/Users/test"));
        let expected = r"C:\Users\test";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_win_path_no_drive() {
        let actual = to_win_path(Path::new("/usr/bin/zsh"));
        let expected = r"\usr\bin\zsh";
        assert_eq!(actual, expected);
    }

    // ---- Oh My Zsh detection tests ----

    #[tokio::test]
    async fn test_detect_oh_my_zsh_installed() {
        let temp = tempfile::TempDir::new().unwrap();
        let omz_dir = temp.path().join(".oh-my-zsh");
        std::fs::create_dir(&omz_dir).unwrap();

        // Temporarily set HOME
        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = detect_oh_my_zsh().await;

        // Restore
        unsafe {
            if let Some(h) = original_home {
                std::env::set_var("HOME", h);
            }
        }

        assert!(matches!(actual, OmzStatus::Installed { .. }));
    }

    #[tokio::test]
    async fn test_detect_oh_my_zsh_not_installed() {
        let temp = tempfile::TempDir::new().unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = detect_oh_my_zsh().await;

        unsafe {
            if let Some(h) = original_home {
                std::env::set_var("HOME", h);
            }
        }

        assert!(matches!(actual, OmzStatus::NotInstalled));
    }

    // ---- Plugin detection tests ----

    #[tokio::test]
    async fn test_detect_autosuggestions_installed() {
        let temp = tempfile::TempDir::new().unwrap();
        let plugin_dir = temp.path().join("plugins").join("zsh-autosuggestions");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let original_custom = std::env::var("ZSH_CUSTOM").ok();
        unsafe {
            std::env::set_var("ZSH_CUSTOM", temp.path());
        }

        let actual = detect_autosuggestions().await;

        unsafe {
            if let Some(c) = original_custom {
                std::env::set_var("ZSH_CUSTOM", c);
            } else {
                std::env::remove_var("ZSH_CUSTOM");
            }
        }

        assert_eq!(actual, PluginStatus::Installed);
    }

    #[tokio::test]
    async fn test_detect_autosuggestions_not_installed() {
        let temp = tempfile::TempDir::new().unwrap();

        let original_custom = std::env::var("ZSH_CUSTOM").ok();
        unsafe {
            std::env::set_var("ZSH_CUSTOM", temp.path());
        }

        let actual = detect_autosuggestions().await;

        unsafe {
            if let Some(c) = original_custom {
                std::env::set_var("ZSH_CUSTOM", c);
            } else {
                std::env::remove_var("ZSH_CUSTOM");
            }
        }

        assert_eq!(actual, PluginStatus::NotInstalled);
    }
}
