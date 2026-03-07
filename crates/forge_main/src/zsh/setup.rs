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

/// Runs all dependency detection functions in parallel and returns aggregated
/// results.
///
/// # Returns
///
/// A `DependencyStatus` containing the status of all dependencies.
pub async fn detect_all_dependencies() -> DependencyStatus {
    let (git, zsh, oh_my_zsh, autosuggestions, syntax_highlighting, fzf) = tokio::join!(
        detect_git(),
        detect_zsh(),
        detect_oh_my_zsh(),
        detect_autosuggestions(),
        detect_syntax_highlighting(),
        detect_fzf(),
    );

    DependencyStatus {
        zsh,
        oh_my_zsh,
        autosuggestions,
        syntax_highlighting,
        fzf,
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
            git: true,
        };

        assert!(!fixture.all_installed());

        let actual = fixture.missing_items();
        let expected = vec![("zsh", "shell")];
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
            git: true,
        };

        let actual = fixture.missing_items();
        let expected = vec![
            ("zsh", "shell"),
            ("Oh My Zsh", "plugin framework"),
            ("zsh-autosuggestions", "plugin"),
            ("zsh-syntax-highlighting", "plugin"),
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
            git: true,
        };

        let actual = fixture.missing_items();
        let expected = vec![("zsh-autosuggestions", "plugin")];
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
