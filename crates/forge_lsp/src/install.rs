use std::path::PathBuf;
use std::process::Command;
use anyhow::{Result, anyhow, Context};
use dirs::data_local_dir;
use tracing::info;
use which::which;

pub fn get_lsp_bin_dir() -> Result<PathBuf> {
    let mut path = data_local_dir().ok_or_else(|| anyhow!("Could not find data local dir"))?;
    path.push("forge");
    path.push("bin");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

#[derive(Debug, Clone)]
pub enum InstallationStrategy {
    Npm { package: String },
    Go { package: String },
    Dotnet { package: String },
}

impl InstallationStrategy {
    pub fn install(&self) -> Result<()> {
        let bin_dir = get_lsp_bin_dir()?;
        match self {
            InstallationStrategy::Npm { package } => {
                info!("Installing {} via npm...", package);
                
                let status = Command::new("npm")
                    .arg("install")
                    .arg("--prefix")
                    .arg(&bin_dir)
                    .args(package.split_whitespace())
                    .status()
                    .context("Failed to run npm install")?;
                
                if !status.success() {
                    return Err(anyhow!("npm install failed"));
                }
            }
            InstallationStrategy::Go { package } => {
                info!("Installing {} via go...", package);
                let status = Command::new("go")
                    .arg("install")
                    .arg(package)
                    .env("GOBIN", &bin_dir)
                    .status()
                    .context("Failed to run go install")?;

                if !status.success() {
                    return Err(anyhow!("go install failed"));
                }
            }
            InstallationStrategy::Dotnet { package } => {
                info!("Installing {} via dotnet...", package);
                let status = Command::new("dotnet")
                    .arg("tool")
                    .arg("install")
                    .arg(package)
                    .arg("--tool-path")
                    .arg(&bin_dir)
                    .status()
                    .context("Failed to run dotnet tool install")?;

                if !status.success() {
                    return Err(anyhow!("dotnet tool install failed"));
                }
            }
        }
        Ok(())
    }
}

pub fn find_or_install(command: &str, strategy: Option<&InstallationStrategy>) -> Result<PathBuf> {
    // 1. Check PATH
    if let Ok(path) = which(command) {
        return Ok(path);
    }

    let bin_dir = get_lsp_bin_dir()?;
    
    // 2. Check local bin (Go/Dotnet style)
    let local_bin = bin_dir.join(command);
    if local_bin.exists() {
        return Ok(local_bin);
    }
    // Windows check
    let local_bin_exe = bin_dir.join(format!("{}.exe", command));
    if local_bin_exe.exists() {
        return Ok(local_bin_exe);
    }

    // 3. Check npm bin
    let npm_bin = bin_dir.join("node_modules").join(".bin").join(command);
    let npm_bin_cmd = bin_dir.join("node_modules").join(".bin").join(format!("{}.cmd", command));
    
    if npm_bin.exists() || npm_bin_cmd.exists() {
        let mut dependencies_satisfied = true;
        if let Some(InstallationStrategy::Npm { package }) = strategy {
             if !check_npm_dependencies(package).unwrap_or(false) {
                 dependencies_satisfied = false;
                 info!("Dependencies for {} missing or incomplete, triggering reinstall...", command);
             }
        }

        if dependencies_satisfied {
            if npm_bin.exists() {
                return Ok(npm_bin);
            }
            if npm_bin_cmd.exists() {
                return Ok(npm_bin_cmd);
            }
        }
    }

    // 4. Install if strategy provided
    if let Some(strategy) = strategy {
        strategy.install()?;
        
        // Re-check locations
        if local_bin.exists() {
            return Ok(local_bin);
        }
        if local_bin_exe.exists() {
            return Ok(local_bin_exe);
        }
        if npm_bin.exists() {
            return Ok(npm_bin);
        }
        if npm_bin_cmd.exists() {
            return Ok(npm_bin_cmd);
        }
    }

    Err(anyhow!("Binary {} not found and installation failed or not supported", command))
}

fn get_package_name(pkg: &str) -> String {
    if pkg.starts_with('@') {
        // Scoped package: @scope/pkg or @scope/pkg@ver
        if let Some(idx) = pkg[1..].find('@') {
            pkg[0..idx+1].to_string()
        } else {
            pkg.to_string()
        }
    } else {
        // Unscoped: pkg or pkg@ver
        if let Some(idx) = pkg.find('@') {
            pkg[0..idx].to_string()
        } else {
            pkg.to_string()
        }
    }
}

fn check_npm_dependencies(package: &str) -> Result<bool> {
    let bin_dir = get_lsp_bin_dir()?;
    let package_json_path = bin_dir.join("package.json");
    
    if !package_json_path.exists() {
        return Ok(false);
    }
    
    let content = std::fs::read_to_string(package_json_path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    
    let dependencies = match json.get("dependencies") {
        Some(deps) => deps.as_object(),
        None => return Ok(false),
    };
    
    let dependencies = match dependencies {
        Some(d) => d,
        None => return Ok(false),
    };
    
    for pkg_part in package.split_whitespace() {
        let pkg_name = get_package_name(pkg_part);
        if !dependencies.contains_key(&pkg_name) {
            return Ok(false);
        }
    }
    
    Ok(true)
}
