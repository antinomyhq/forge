//! Tool installation functions (fzf, bat, fd).
//!
//! Handles installation of CLI tools via package managers or GitHub releases,
//! including version checking, archive extraction, and binary deployment.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tokio::process::Command;

use super::detect::{detect_bat, detect_fd, detect_fzf};
use super::install_zsh::LinuxPackageManager;
use super::libc::{LibcType, detect_libc_type};
use super::platform::{Arch, Platform};
use super::types::*;
use super::util::*;
use super::{BAT_MIN_VERSION, FD_MIN_VERSION, FZF_MIN_VERSION};

/// Installs fzf (fuzzy finder) using package manager or GitHub releases.
///
/// Tries package manager first (which checks version requirements before
/// installing). Falls back to GitHub releases if package manager unavailable or
/// version too old.
pub async fn install_fzf(platform: Platform, sudo: &SudoCapability) -> Result<()> {
    // Try package manager first (version is checked before installing)
    // NOTE: Use Err() not bail!() — bail! returns from the function immediately,
    // preventing the GitHub release fallback below from running.
    let pkg_mgr_result = try_install_via_package_manager("fzf", platform, sudo).await;

    // If package manager succeeded, verify installation and version
    if pkg_mgr_result.is_ok() {
        let status = detect_fzf().await;
        if matches!(status, FzfStatus::Found { meets_minimum: true, .. }) {
            return Ok(());
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
    // NOTE: Use Err() not bail!() — bail! returns from the function immediately,
    // preventing the GitHub release fallback below from running.
    let pkg_mgr_result = try_install_via_package_manager("bat", platform, sudo).await;

    // If package manager succeeded, verify installation and version
    if pkg_mgr_result.is_ok() {
        let status = detect_bat().await;
        if matches!(status, BatStatus::Installed { meets_minimum: true, .. }) {
            return Ok(());
        }
    }

    // Fall back to GitHub releases (pkg mgr unavailable or version too old)
    install_sharkdp_tool_from_github("bat", "sharkdp/bat", "0.25.0", platform).await
}

/// Installs fd (file finder) using package manager or GitHub releases.
///
/// Tries package manager first (which checks version requirements before
/// installing). Falls back to GitHub releases if package manager unavailable or
/// version too old.
pub async fn install_fd(platform: Platform, sudo: &SudoCapability) -> Result<()> {
    // Try package manager first (version is checked before installing)
    // NOTE: Use Err() not bail!() — bail! returns from the function immediately,
    // preventing the GitHub release fallback below from running.
    let pkg_mgr_result = try_install_via_package_manager("fd", platform, sudo).await;

    // If package manager succeeded, verify installation and version
    if pkg_mgr_result.is_ok() {
        let status = detect_fd().await;
        if matches!(status, FdStatus::Installed { meets_minimum: true, .. }) {
            return Ok(());
        }
    }

    // Fall back to GitHub releases (pkg mgr unavailable or version too old)
    install_sharkdp_tool_from_github("fd", "sharkdp/fd", "10.1.0", platform).await
}

/// Tries to install a tool using the platform's native package manager.
///
/// Returns `Ok(())` if the package manager ran successfully (the caller should
/// still verify the installed version). Returns `Err` if no package manager is
/// available or the install command failed -- the caller should fall back to
/// GitHub releases.
async fn try_install_via_package_manager(
    tool: &str,
    platform: Platform,
    sudo: &SudoCapability,
) -> Result<()> {
    match platform {
        Platform::Linux => install_via_package_manager_linux(tool, sudo).await,
        Platform::MacOS => install_via_brew(tool).await,
        Platform::Android => install_via_pkg(tool).await,
        Platform::Windows => Err(anyhow::anyhow!("No package manager on Windows")),
    }
}

/// Installs a tool via Homebrew on macOS.
async fn install_via_brew(tool: &str) -> Result<()> {
    if !command_exists("brew").await {
        bail!("brew not found");
    }
    let status = Command::new("brew")
        .args(["install", tool])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await?;
    if status.success() {
        Ok(())
    } else {
        bail!("brew install {} failed", tool)
    }
}

/// Installs a tool via pkg on Android (Termux).
async fn install_via_pkg(tool: &str) -> Result<()> {
    if !command_exists("pkg").await {
        bail!("pkg not found");
    }
    let status = Command::new("pkg")
        .args(["install", "-y", tool])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await?;
    if status.success() {
        Ok(())
    } else {
        bail!("pkg install {} failed", tool)
    }
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

            if let Some(available_version) = mgr.query_available_version(package_name).await
                && !version_gte(&available_version, min_version)
            {
                bail!(
                    "Package manager has {} {} but {} or higher required",
                    tool,
                    available_version,
                    min_version
                );
            }
            // Version is good, proceed with installation

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
    let asset_pattern = platform.fzf_asset_pattern();

    let version = get_latest_release_with_binary("junegunn/fzf", asset_pattern, "0.56.3").await;

    let url = construct_fzf_url(&version, platform)?;
    let archive_type = match platform.archive_ext() {
        "zip" => ArchiveType::Zip,
        _ => ArchiveType::TarGz,
    };

    download_extract_and_install(&url, "fzf", archive_type, false).await
}

/// Installs a sharkdp tool (bat, fd) from GitHub releases.
///
/// Both bat and fd follow the same naming convention:
/// `{tool}-v{version}-{target}.{ext}` with nested archive layout.
///
/// # Arguments
/// * `tool` - Tool name (e.g., "bat", "fd")
/// * `repo` - GitHub repository (e.g., "sharkdp/bat")
/// * `fallback_version` - Version to use if GitHub API is unavailable
/// * `platform` - Target platform
async fn install_sharkdp_tool_from_github(
    tool: &str,
    repo: &str,
    fallback_version: &str,
    platform: Platform,
) -> Result<()> {
    let target = construct_rust_target(platform).await?;

    let version = get_latest_release_with_binary(repo, &target, fallback_version).await;
    let ext = platform.archive_ext();
    let archive_type = match ext {
        "zip" => ArchiveType::Zip,
        _ => ArchiveType::TarGz,
    };
    let url = format!(
        "https://github.com/{}/releases/download/v{}/{}-v{}-{}.{}",
        repo, version, tool, version, target, ext
    );

    download_extract_and_install(&url, tool, archive_type, true).await
}

/// Minimal struct for parsing GitHub release API response.
#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

/// Minimal struct for parsing GitHub asset info.
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

/// Archive type for tool downloads.
#[derive(Debug, Clone, Copy)]
enum ArchiveType {
    TarGz,
    Zip,
}

/// Downloads, extracts, and installs a tool binary to `~/.local/bin`.
///
/// Creates a temporary directory for the download, extracts the archive,
/// copies the binary to `~/.local/bin`, and cleans up the temp directory.
///
/// # Arguments
/// * `url` - Download URL for the archive
/// * `tool_name` - Name of the binary to find in the archive
/// * `archive_type` - Whether the archive is tar.gz or zip
/// * `nested` - If true, searches subdirectories for the binary (e.g., bat/fd
///   archives)
async fn download_extract_and_install(
    url: &str,
    tool_name: &str,
    archive_type: ArchiveType,
    nested: bool,
) -> Result<()> {
    let temp_dir = std::env::temp_dir().join(format!("forge-{}-download", tool_name));
    tokio::fs::create_dir_all(&temp_dir).await?;
    let _cleanup = TempDirCleanup(temp_dir.clone());

    // Download archive
    let response = reqwest::get(url).await.context("Failed to download tool")?;
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
    extract_archive(&archive_path, &temp_dir, archive_type).await?;

    // Find binary in extracted files
    let binary_path = find_binary_in_dir(&temp_dir, tool_name, nested).await?;

    // Install to ~/.local/bin
    install_binary_to_local_bin(&binary_path, tool_name).await?;

    Ok(())
}

/// Extracts an archive to the given destination directory.
async fn extract_archive(
    archive_path: &Path,
    dest_dir: &Path,
    archive_type: ArchiveType,
) -> Result<()> {
    match archive_type {
        ArchiveType::TarGz => {
            let status = Command::new("tar")
                .args(["-xzf", &path_str(archive_path), "-C", &path_str(dest_dir)])
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
                            dest_dir.display()
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
                    .args(["-q", &path_str(archive_path), "-d", &path_str(dest_dir)])
                    .status()
                    .await?;
                if !status.success() {
                    bail!("Failed to extract zip archive");
                }
            }
        }
    }
    Ok(())
}

/// Locates the tool binary inside an extracted archive directory.
///
/// If `nested` is true, searches one level of subdirectories (for archives
/// like bat/fd that wrap contents in a folder). Otherwise looks at the top
/// level.
async fn find_binary_in_dir(dir: &Path, tool_name: &str, nested: bool) -> Result<PathBuf> {
    let binary_name = if cfg!(target_os = "windows") {
        format!("{}.exe", tool_name)
    } else {
        tool_name.to_string()
    };

    if nested {
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let candidate = entry.path().join(&binary_name);
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }
        bail!(
            "Binary '{}' not found in nested archive structure",
            tool_name
        );
    } else {
        let candidate = dir.join(&binary_name);
        if candidate.exists() {
            Ok(candidate)
        } else {
            bail!("Binary '{}' not found in flat archive structure", tool_name);
        }
    }
}

/// Installs a binary to `~/.local/bin` with executable permissions.
async fn install_binary_to_local_bin(binary_path: &Path, name: &str) -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let local_bin = PathBuf::from(home).join(".local").join("bin");
    tokio::fs::create_dir_all(&local_bin).await?;

    let dest_name = if cfg!(target_os = "windows") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };
    let dest = local_bin.join(dest_name);
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
    let arch = Arch::detect()?;
    Ok(format!(
        "https://github.com/junegunn/fzf/releases/download/v{}/fzf-{}-{}_{}.{}",
        version,
        version,
        platform.fzf_os(),
        arch.as_go(),
        platform.archive_ext()
    ))
}

/// Constructs a Rust target triple for bat/fd downloads.
async fn construct_rust_target(platform: Platform) -> Result<String> {
    let arch = Arch::detect()?;
    match platform {
        Platform::Linux => {
            let libc = detect_libc_type().await.unwrap_or(LibcType::Musl);
            let libc_suffix = match libc {
                LibcType::Musl => "musl",
                LibcType::Gnu => "gnu",
            };
            Ok(format!("{}-unknown-linux-{}", arch.as_rust(), libc_suffix))
        }
        Platform::MacOS => Ok(format!("{}-apple-darwin", arch.as_rust())),
        Platform::Windows => Ok(format!("{}-pc-windows-msvc", arch.as_rust())),
        Platform::Android => Ok("aarch64-unknown-linux-musl".to_string()),
    }
}
