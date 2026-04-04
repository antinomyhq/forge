//! PowerShell plugin and theme generation, setup, and diagnostics.
//! Embeds .ps1 files from shell-plugin/pwsh/ at compile time.

use std::path::PathBuf;

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};

use crate::shell::normalize_script;
use crate::shell::setup::{self, ShellSetupConfig};

/// Embeds shell plugin files for PowerShell integration.
static PWSH_PLUGIN_LIB: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/../../shell-plugin/pwsh/lib");

/// Generates the complete PowerShell plugin by combining embedded files.
pub fn generate_powershell_plugin() -> Result<String> {
    let mut output = String::new();

    // Header
    output.push_str("# Forge PowerShell Plugin (auto-generated)\n");
    output.push_str("# Do not edit - regenerate with: forge powershell plugin\n\n");

    // Concatenate all .ps1 files from the embedded lib directory
    collect_ps1_files(&PWSH_PLUGIN_LIB, &mut output);

    // Mark plugin as loaded
    output.push_str("\n$env:_FORGE_PLUGIN_LOADED = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()\n");

    Ok(normalize_script(&output))
}

/// Recursively collects and appends all .ps1 files from a directory.
fn collect_ps1_files(dir: &Dir<'_>, output: &mut String) {
    // Process files in this directory first
    for file in dir.files() {
        if let Some(ext) = file.path().extension()
            && ext == "ps1"
                && let Some(contents) = file.contents_utf8() {
                    output.push_str(&format!("\n# --- {} ---\n", file.path().display()));
                    // Strip comment-only lines to reduce size
                    for line in contents.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            continue;
                        }
                        output.push_str(line);
                        output.push('\n');
                    }
                }
    }

    // Recurse into subdirectories
    for subdir in dir.dirs() {
        collect_ps1_files(subdir, output);
    }
}

/// Generates the PowerShell theme script.
pub fn generate_powershell_theme() -> Result<String> {
    const THEME_RAW: &str = include_str!("../../../../shell-plugin/pwsh/forge-theme.ps1");
    let mut output = normalize_script(THEME_RAW);
    output.push_str("\n$env:_FORGE_THEME_LOADED = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()\n");
    Ok(output)
}

fn powershell_format_export(key: &str, value: &str) -> String {
    format!("$env:{} = \"{}\"", key, value)
}

/// Result of PowerShell setup operation.
#[derive(Debug)]
pub struct PowerShellSetupResult {
    pub message: String,
    pub backup_path: Option<PathBuf>,
}

/// Sets up PowerShell integration by modifying `$PROFILE`.
pub fn setup_powershell_integration(
    disable_nerd_font: bool,
    forge_editor: Option<&str>,
) -> Result<PowerShellSetupResult> {
    const INIT_RAW: &str = include_str!("../../../../shell-plugin/pwsh/forge-setup.ps1");
    let init_content = normalize_script(INIT_RAW);

    let profile_path = find_powershell_profile()?;

    let config = ShellSetupConfig {
        start_marker: "# >>> forge initialize >>>",
        end_marker: "# <<< forge initialize <<<",
        profile_path: &profile_path,
        init_content: &init_content,
        disable_nerd_font,
        forge_editor,
        format_export: powershell_format_export,
    };

    let result = setup::setup_shell_integration(&config)?;

    Ok(PowerShellSetupResult { message: result.message, backup_path: result.backup_path })
}

/// Finds the PowerShell profile path.
///
/// Tries pwsh (PowerShell 7+) first, falls back to Windows PowerShell 5.1.
fn find_powershell_profile() -> Result<PathBuf> {
    // Try pwsh first (cross-platform PowerShell 7+)
    if let Ok(output) = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-Command", "$PROFILE"])
        .output()
        && output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }

    // Fall back to Windows PowerShell
    if cfg!(target_os = "windows")
        && let Ok(output) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "$PROFILE"])
            .output()
            && output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(PathBuf::from(path));
                }
            }

    // Final fallback: construct the standard path
    let home = if cfg!(target_os = "windows") {
        std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))
    } else {
        std::env::var("HOME")
    }
    .context("Could not determine home directory")?;

    let profile_dir = if cfg!(target_os = "windows") {
        PathBuf::from(&home).join("Documents").join("PowerShell")
    } else {
        PathBuf::from(&home).join(".config").join("powershell")
    };

    Ok(profile_dir.join("Microsoft.PowerShell_profile.ps1"))
}

/// Runs the PowerShell doctor diagnostics script.
pub fn run_powershell_doctor() -> Result<()> {
    let doctor_script = generate_doctor_script();
    execute_powershell_script(&doctor_script)
}

/// Runs the PowerShell keyboard shortcuts display.
pub fn run_powershell_keyboard() -> Result<()> {
    let keyboard_script = generate_keyboard_script();
    execute_powershell_script(&keyboard_script)
}

fn execute_powershell_script(script: &str) -> Result<()> {
    // Prefer pwsh, fall back to powershell.exe
    let shell = if std::process::Command::new("pwsh")
        .arg("--version")
        .output()
        .is_ok()
    {
        "pwsh"
    } else {
        "powershell"
    };

    let output = std::process::Command::new(shell)
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .context(format!("Failed to execute {} script", shell))?;

    if !output.success() {
        anyhow::bail!("Script exited with code: {:?}", output.code());
    }

    Ok(())
}

fn generate_doctor_script() -> String {
    let e = r#"[char]27"#;
    format!(
        r#"
$e = {e}
Write-Host "$($e)[1mForge PowerShell Doctor$($e)[0m"
Write-Host ""

# PowerShell version
$psVer = $PSVersionTable.PSVersion
Write-Host "$($e)[32m[OK]$($e)[0m PowerShell $psVer"

# PSReadLine version
$psr = Get-Module PSReadLine -ErrorAction SilentlyContinue
if ($psr) {{
    $v = $psr.Version
    if ($v -ge [version]"2.2.0") {{
        Write-Host "$($e)[32m[OK]$($e)[0m PSReadLine $v"
    }} else {{
        Write-Host "$($e)[31m[!!]$($e)[0m PSReadLine $v (need >= 2.2.0, run: Install-Module PSReadLine -Force)"
    }}
}} else {{
    Write-Host "$($e)[31m[!!]$($e)[0m PSReadLine not loaded"
}}

# fzf
if (Get-Command fzf -ErrorAction SilentlyContinue) {{
    $fzfVer = & fzf --version 2>$null
    Write-Host "$($e)[32m[OK]$($e)[0m fzf $fzfVer"
}} else {{
    Write-Host "$($e)[33m[--]$($e)[0m fzf not found (optional, for interactive selection)"
}}

# fd
if (Get-Command fd -ErrorAction SilentlyContinue) {{
    Write-Host "$($e)[32m[OK]$($e)[0m fd found"
}} else {{
    Write-Host "$($e)[33m[--]$($e)[0m fd not found (optional, for file search)"
}}

# bat
if (Get-Command bat -ErrorAction SilentlyContinue) {{
    Write-Host "$($e)[32m[OK]$($e)[0m bat found"
}} else {{
    Write-Host "$($e)[33m[--]$($e)[0m bat not found (optional, for preview)"
}}

# forge binary
if (Get-Command forge -ErrorAction SilentlyContinue) {{
    $forgeVer = & forge --version 2>$null
    Write-Host "$($e)[32m[OK]$($e)[0m forge $forgeVer"
}} else {{
    Write-Host "$($e)[31m[!!]$($e)[0m forge not found in PATH"
}}

# Plugin loaded
if ($env:_FORGE_PLUGIN_LOADED) {{
    Write-Host "$($e)[32m[OK]$($e)[0m Plugin loaded (at $env:_FORGE_PLUGIN_LOADED)"
}} else {{
    Write-Host "$($e)[33m[--]$($e)[0m Plugin not loaded (run: forge powershell setup)"
}}

# Theme loaded
if ($env:_FORGE_THEME_LOADED) {{
    Write-Host "$($e)[32m[OK]$($e)[0m Theme loaded (at $env:_FORGE_THEME_LOADED)"
}} else {{
    Write-Host "$($e)[33m[--]$($e)[0m Theme not loaded (run: forge powershell setup)"
}}

# Terminal ANSI support
if ($env:WT_SESSION) {{
    Write-Host "$($e)[32m[OK]$($e)[0m Windows Terminal detected"
}} elseif ($env:TERM_PROGRAM) {{
    Write-Host "$($e)[32m[OK]$($e)[0m Terminal: $env:TERM_PROGRAM"
}} else {{
    Write-Host "$($e)[33m[--]$($e)[0m Terminal not detected (ANSI colors may not work)"
}}

Write-Host ""
Write-Host "Done."
"#
    )
}

fn generate_keyboard_script() -> String {
    let e = r#"[char]27"#;
    format!(
        r#"
$e = {e}
Write-Host "$($e)[1mForge PowerShell Keyboard Shortcuts$($e)[0m"
Write-Host ""
Write-Host "$($e)[36mEnter$($e)[0m    Execute :command or normal command"
Write-Host "$($e)[36mTab$($e)[0m      Complete :command or @file (with fzf)"
Write-Host ""
Write-Host "$($e)[1mColon Commands:$($e)[0m"
Write-Host "  $($e)[33m: <text>$($e)[0m           Send prompt to default agent"
Write-Host "  $($e)[33m:<agent> <text>$($e)[0m     Send prompt to specific agent"
Write-Host "  $($e)[33m:new$($e)[0m / $($e)[33m:n$($e)[0m          Start new conversation"
Write-Host "  $($e)[33m:info$($e)[0m / $($e)[33m:i$($e)[0m         Show session info"
Write-Host "  $($e)[33m:agent$($e)[0m / $($e)[33m:a$($e)[0m        Switch agent"
Write-Host "  $($e)[33m:conversation$($e)[0m / $($e)[33m:c$($e)[0m  Switch conversation"
Write-Host "  $($e)[33m:session-model$($e)[0m / $($e)[33m:m$($e)[0m Session model override"
Write-Host "  $($e)[33m:config-model$($e)[0m / $($e)[33m:cm$($e)[0m Global model config"
Write-Host "  $($e)[33m:reasoning-effort$($e)[0m / $($e)[33m:re$($e)[0m  Reasoning effort"
Write-Host "  $($e)[33m:commit$($e)[0m            Generate commit message"
Write-Host "  $($e)[33m:suggest$($e)[0m / $($e)[33m:s$($e)[0m      Suggest shell command"
Write-Host "  $($e)[33m:edit$($e)[0m              Open editor for prompt"
Write-Host "  $($e)[33m:copy$($e)[0m              Copy last response"
Write-Host "  $($e)[33m:doctor$($e)[0m            Run diagnostics"
Write-Host ""
Write-Host "$($e)[90mSee all commands: forge list commands$($e)[0m"
"#
    )
}
