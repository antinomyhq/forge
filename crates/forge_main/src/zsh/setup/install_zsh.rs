//! ZSH installation functions.
//!
//! Handles platform-specific zsh installation (Linux, macOS, Android,
//! Windows/Git Bash) including MSYS2 package management, extraction methods,
//! and shell configuration.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tokio::process::Command;

use super::platform::Platform;
use super::types::SudoCapability;
use super::util::*;
use super::{MSYS2_BASE, MSYS2_PKGS};

/// Installs zsh using the appropriate method for the detected platform.
///
/// When `reinstall` is true, forces a reinstallation (e.g., for broken
/// modules).
///
/// # Errors
///
/// Returns error if no supported package manager is found or installation
/// fails.
pub async fn install_zsh(
    platform: Platform,
    sudo: &SudoCapability,
    reinstall: bool,
) -> Result<()> {
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
pub(super) enum LinuxPackageManager {
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
    pub(super) fn install_args<S: AsRef<str>>(&self, packages: &[S]) -> Vec<String> {
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
    pub(super) fn all() -> &'static [Self] {
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
    pub(super) fn fzf_package_name(&self) -> &'static str {
        "fzf"
    }

    /// Returns the package name for bat.
    ///
    /// On Debian/Ubuntu, the package is named "bat" (not "batcat").
    /// The binary is installed as "batcat" to avoid conflicts.
    pub(super) fn bat_package_name(&self) -> &'static str {
        "bat"
    }

    /// Returns the package name for fd.
    ///
    /// On Debian/Ubuntu, the package is named "fd-find" due to naming
    /// conflicts.
    pub(super) fn fd_package_name(&self) -> &'static str {
        match self {
            Self::AptGet => "fd-find",
            _ => "fd",
        }
    }

    /// Queries the available version of a package from the package manager.
    ///
    /// Returns None if the package is not available or version cannot be
    /// determined.
    pub(super) async fn query_available_version(&self, package: &str) -> Option<String> {
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

    let fpath_block = include_str!("../scripts/zshenv_fpath_block.sh");

    content.push_str(fpath_block);
    tokio::fs::write(&zshenv_path, &content)
        .await
        .context("Failed to write ~/.zshenv")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

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
}
