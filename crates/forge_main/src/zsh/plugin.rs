use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use clap::CommandFactory;
use clap_complete::generate;
use clap_complete::shells::Zsh;
use include_dir::{Dir, include_dir};

use crate::cli::Cli;

/// Embeds shell plugin files for zsh integration
static ZSH_PLUGIN_LIB: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../shell-plugin/lib");

/// Generates the complete zsh plugin by combining embedded files and clap
/// completions
pub fn generate_zsh_plugin() -> Result<String> {
    let mut output = String::new();

    // Iterate through all embedded files in shell-plugin/lib, stripping comments
    // and empty lines. All files in this directory are .zsh files.
    for file in forge_embed::files(&ZSH_PLUGIN_LIB) {
        let content = super::normalize_script(std::str::from_utf8(file.contents())?);
        for line in content.lines() {
            let trimmed = line.trim();
            // Skip empty lines and comment lines
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                output.push_str(line);
                output.push('\n');
            }
        }
    }

    // Generate clap completions for the CLI
    let mut cmd = Cli::command();
    let mut completions = Vec::new();
    generate(Zsh, &mut cmd, "forge", &mut completions);

    // Append completions to the output with clear separator
    let completions_str = String::from_utf8(completions)?;
    output.push_str("\n# --- Clap Completions ---\n");
    output.push_str(&completions_str);

    // Set environment variable to indicate plugin is loaded (with timestamp)
    output.push_str("\n_FORGE_PLUGIN_LOADED=$(date +%s)\n");

    Ok(output)
}

/// Generates the ZSH theme for Forge
pub fn generate_zsh_theme() -> Result<String> {
    let mut content =
        super::normalize_script(include_str!("../../../../shell-plugin/forge.theme.zsh"));

    // Set environment variable to indicate theme is loaded (with timestamp)
    content.push_str("\n_FORGE_THEME_LOADED=$(date +%s)\n");

    Ok(content)
}

/// Creates a temporary zsh script file for Windows execution
fn create_temp_zsh_script(script_content: &str) -> Result<(tempfile::TempDir, PathBuf)> {
    use std::io::Write;

    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let script_path = temp_dir.path().join("forge_script.zsh");
    let mut file = fs::File::create(&script_path).context("Failed to create temp script file")?;
    file.write_all(script_content.as_bytes())
        .context("Failed to write temp script")?;

    Ok((temp_dir, script_path))
}

/// Executes a ZSH script with streaming output
///
/// # Arguments
///
/// * `script_content` - The ZSH script content to execute
/// * `script_name` - Descriptive name for the script (used in error messages)
///
/// # Errors
///
/// Returns error if the script cannot be executed, if output streaming fails,
/// or if the script exits with a non-zero status code
fn execute_zsh_script_with_streaming(script_content: &str, script_name: &str) -> Result<()> {
    let script_content = super::normalize_script(script_content);

    // On Unix, pass the script via `zsh -c`. Command::arg() uses execve,
    // which forwards arguments directly without shell interpretation, so
    // embedded quotes are safe.
    //
    // On Windows, we write the script to a temp file and run `zsh -f <file>`
    // instead. A temp file is necessary because:
    //   1. CI has core.autocrlf=true, so checked-out files contain CRLF; writing
    //      through normalize_script ensures the temp file has LF.
    //   2. CreateProcess mangles quotes, so passing the script via -c corrupts any
    //      embedded quoting.
    //   3. Piping via stdin is unreliable -- Windows caps pipe buffer size, which
    //      can truncate or block on larger scripts.
    // The -f flag also prevents ~/.zshrc from loading during execution.
    let (_temp_dir, mut child) = if cfg!(windows) {
        let (temp_dir, script_path) = create_temp_zsh_script(&script_content)?;
        let child = std::process::Command::new("zsh")
            // -f: don't load ~/.zshrc (prevents theme loading during doctor)
            .arg("-f")
            .arg(script_path.to_string_lossy().as_ref())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!("Failed to execute zsh {} script", script_name))?;
        // Keep temp_dir alive by boxing it in the tuple
        (Some(temp_dir), child)
    } else {
        let child = std::process::Command::new("zsh")
            .arg("-c")
            .arg(&script_content)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!("Failed to execute zsh {} script", script_name))?;
        (None, child)
    };

    // Get stdout and stderr handles
    let stdout = child.stdout.take().context("Failed to capture stdout")?;
    let stderr = child.stderr.take().context("Failed to capture stderr")?;

    // Use scoped threads for safer streaming with automatic joining
    std::thread::scope(|s| {
        // Stream stdout line by line
        s.spawn(|| {
            let stdout_reader = BufReader::new(stdout);
            for line in stdout_reader.lines() {
                match line {
                    Ok(line) => println!("{}", line),
                    Err(e) => eprintln!("Error reading stdout: {}", e),
                }
            }
        });

        // Stream stderr line by line
        s.spawn(|| {
            let stderr_reader = BufReader::new(stderr);
            for line in stderr_reader.lines() {
                match line {
                    Ok(line) => eprintln!("{}", line),
                    Err(e) => eprintln!("Error reading stderr: {}", e),
                }
            }
        });
    });

    // Wait for the child process to complete
    let status = child
        .wait()
        .context(format!("Failed to wait for zsh {} script", script_name))?;

    if !status.success() {
        let exit_code = status
            .code()
            .map_or_else(|| "unknown".to_string(), |code| code.to_string());

        anyhow::bail!(
            "ZSH {} script failed with exit code: {}",
            script_name,
            exit_code
        );
    }

    Ok(())
}

/// Runs diagnostics on the ZSH shell environment with streaming output
///
/// # Errors
///
/// Returns error if the doctor script cannot be executed
pub fn run_zsh_doctor() -> Result<()> {
    let script_content = include_str!("../../../../shell-plugin/doctor.zsh");
    execute_zsh_script_with_streaming(script_content, "doctor")
}

/// Shows ZSH keyboard shortcuts with streaming output
///
/// # Errors
///
/// Returns error if the keyboard script cannot be executed
pub fn run_zsh_keyboard() -> Result<()> {
    let script_content = include_str!("../../../../shell-plugin/keyboard.zsh");
    execute_zsh_script_with_streaming(script_content, "keyboard")
}

/// Represents the state of markers in a file
/// Result of ZSH setup operation
#[derive(Debug)]
pub struct ZshSetupResult {
    /// Status message describing what was done
    pub message: String,
    /// Path to backup file if one was created
    pub backup_path: Option<PathBuf>,
}

fn zsh_format_export(key: &str, value: &str) -> String {
    format!("export {}=\"{}\"", key, value)
}

/// Sets up ZSH integration with optional nerd font and editor configuration
///
/// # Arguments
///
/// * `disable_nerd_font` - If true, adds NERD_FONT=0 to .zshrc
/// * `forge_editor` - If Some(editor), adds FORGE_EDITOR export to .zshrc
///
/// # Errors
///
/// Returns error if:
/// - The HOME environment variable is not set
/// - The .zshrc file cannot be read or written
/// - Invalid forge markers are found (incomplete or incorrectly ordered)
/// - A backup of the existing .zshrc cannot be created
pub fn setup_zsh_integration(
    disable_nerd_font: bool,
    forge_editor: Option<&str>,
) -> Result<ZshSetupResult> {
    const FORGE_INIT_CONFIG_RAW: &str = include_str!("../../../../shell-plugin/forge.setup.zsh");
    let forge_init_config = super::normalize_script(FORGE_INIT_CONFIG_RAW);

    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let zdotdir = std::env::var("ZDOTDIR").unwrap_or_else(|_| home.clone());
    let zshrc_path = PathBuf::from(&zdotdir).join(".zshrc");

    let config = crate::shell::setup::ShellSetupConfig {
        start_marker: "# >>> forge initialize >>>",
        end_marker: "# <<< forge initialize <<<",
        profile_path: &zshrc_path,
        init_content: &forge_init_config,
        disable_nerd_font,
        forge_editor,
        format_export: zsh_format_export,
    };

    let result = crate::shell::setup::setup_shell_integration(&config)?;

    Ok(ZshSetupResult {
        message: result.message,
        backup_path: result.backup_path,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{LazyLock, Mutex};

    use super::*;

    // Mutex to ensure tests that modify environment variables run serially
    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    /// Test that the doctor script executes and streams output
    /// Note: The script may fail with non-zero exit code in test environment
    /// (e.g., plugin not loaded), or zsh may not be available in CI
    #[test]
    fn test_run_zsh_doctor_streaming() {
        // SAFETY: No mutex needed for single test - setting env var for test isolation
        unsafe {
            std::env::set_var("FORGE_SKIP_INTERACTIVE", "1");
        }

        let actual = run_zsh_doctor();

        // Clean up
        // SAFETY: No mutex needed for single test - removing env var after test
        unsafe {
            std::env::remove_var("FORGE_SKIP_INTERACTIVE");
        }

        // The doctor script runs successfully even if it reports failures
        // (failures are expected in test environment where plugin isn't loaded)
        // Also accept cases where zsh is not available in CI environment
        match actual {
            Ok(_) => {
                // Success case
            }
            Err(e) => {
                // Check if it's a non-zero exit code error or zsh not available (both expected
                // in tests)
                let error_msg = e.to_string();
                assert!(
                    error_msg.contains("exit code") || error_msg.contains("Failed to execute"),
                    "Unexpected error: {}",
                    error_msg
                );
            }
        }
    }

    #[test]
    fn test_setup_zsh_integration_without_nerd_font_config() {
        use tempfile::TempDir;

        // Lock to prevent parallel test execution that modifies env vars
        let _guard = ENV_LOCK.lock().unwrap();

        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let zshrc_path = temp_dir.path().join(".zshrc");

        // Set HOME to temp directory
        let original_home = std::env::var("HOME").ok();
        let original_zdotdir = std::env::var("ZDOTDIR").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::remove_var("ZDOTDIR");
        }

        // Run setup without nerd font config
        let actual = setup_zsh_integration(false, None);

        // Restore environment first
        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(zdotdir) = original_zdotdir {
                std::env::set_var("ZDOTDIR", zdotdir);
            } else {
                std::env::remove_var("ZDOTDIR");
            }
        }

        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        // Read the generated .zshrc
        assert!(
            zshrc_path.exists(),
            "zshrc file should be created at {:?}",
            zshrc_path
        );
        let content = fs::read_to_string(&zshrc_path).expect("Should be able to read zshrc");

        // Should not contain NERD_FONT=0
        assert!(!content.contains("NERD_FONT=0"));

        // Should contain the markers
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));
    }

    #[test]
    fn test_setup_zsh_integration_with_nerd_font_disabled() {
        use tempfile::TempDir;

        // Lock to prevent parallel test execution that modifies env vars
        let _guard = ENV_LOCK.lock().unwrap();

        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let zshrc_path = temp_dir.path().join(".zshrc");

        // Set HOME to temp directory
        let original_home = std::env::var("HOME").ok();
        let original_zdotdir = std::env::var("ZDOTDIR").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("ZDOTDIR", temp_dir.path());
        }

        // Run setup with nerd font disabled
        let actual = setup_zsh_integration(true, None);
        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        // Read the generated .zshrc
        assert!(zshrc_path.exists(), "zshrc file should be created");
        let content = fs::read_to_string(&zshrc_path).expect("Should be able to read zshrc");

        // Should contain NERD_FONT=0 with explanatory comments
        assert!(
            content.contains("export NERD_FONT=\"0\""),
            "Content should contain NERD_FONT=0:\n{}",
            content
        );
        assert!(
            content.contains(
                "# Disable Nerd Fonts (set during setup - icons not displaying correctly)"
            ),
            "Should contain explanation comment"
        );
        assert!(content.contains("# To re-enable: remove this line and install a Nerd Font from https://www.nerdfonts.com/"), "Should contain re-enable instructions");

        // Should contain the markers
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));

        // Restore environment
        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            }
            if let Some(zdotdir) = original_zdotdir {
                std::env::set_var("ZDOTDIR", zdotdir);
            }
        }
    }

    #[test]
    fn test_setup_zsh_integration_with_editor() {
        use tempfile::TempDir;

        // Lock to prevent parallel test execution that modifies env vars
        let _guard = ENV_LOCK.lock().unwrap();

        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let zshrc_path = temp_dir.path().join(".zshrc");

        // Set HOME to temp directory
        let original_home = std::env::var("HOME").ok();
        let original_zdotdir = std::env::var("ZDOTDIR").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::remove_var("ZDOTDIR");
        }

        // Run setup with editor configuration
        let actual = setup_zsh_integration(false, Some("code --wait"));

        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        // Read the generated .zshrc
        assert!(zshrc_path.exists(), "zshrc file should be created");
        let content = fs::read_to_string(&zshrc_path).expect("Should be able to read zshrc");

        // Should contain FORGE_EDITOR with explanatory comments
        assert!(
            content.contains("export FORGE_EDITOR=\"code --wait\""),
            "Content should contain FORGE_EDITOR:\n{}",
            content
        );
        assert!(
            content.contains("# Editor for editing prompts (set during setup)"),
            "Should contain editor explanation comment"
        );
        assert!(
            content.contains("# To change: update FORGE_EDITOR or remove to use $EDITOR"),
            "Should contain editor change instructions"
        );

        // Should contain the markers
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));

        // Restore environment
        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(zdotdir) = original_zdotdir {
                std::env::set_var("ZDOTDIR", zdotdir);
            } else {
                std::env::remove_var("ZDOTDIR");
            }
        }
    }

    #[test]
    fn test_setup_zsh_integration_with_both_configs() {
        use tempfile::TempDir;

        // Lock to prevent parallel test execution that modifies env vars
        let _guard = ENV_LOCK.lock().unwrap();

        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let zshrc_path = temp_dir.path().join(".zshrc");

        // Set HOME to temp directory
        let original_home = std::env::var("HOME").ok();
        let original_zdotdir = std::env::var("ZDOTDIR").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("ZDOTDIR", temp_dir.path());
        }

        // Run setup with both nerd font disabled and editor configured
        let actual = setup_zsh_integration(true, Some("vim"));
        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        // Read the generated .zshrc
        assert!(zshrc_path.exists(), "zshrc file should be created");
        let content = fs::read_to_string(&zshrc_path).expect("Should be able to read zshrc");

        // Should contain both configurations
        assert!(
            content.contains("export NERD_FONT=\"0\""),
            "Content should contain NERD_FONT=0:\n{}",
            content
        );
        assert!(
            content.contains("export FORGE_EDITOR=\"vim\""),
            "Content should contain FORGE_EDITOR:\n{}",
            content
        );

        // Should contain the markers
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));

        // Restore environment
        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            }
            if let Some(zdotdir) = original_zdotdir {
                std::env::set_var("ZDOTDIR", zdotdir);
            }
        }
    }

    #[test]
    fn test_setup_zsh_integration_updates_existing_markers() {
        use tempfile::TempDir;

        // Lock to prevent parallel test execution that modifies env vars
        let _guard = ENV_LOCK.lock().unwrap();

        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let zshrc_path = temp_dir.path().join(".zshrc");

        // Set HOME to temp directory
        let original_home = std::env::var("HOME").ok();
        let original_zdotdir = std::env::var("ZDOTDIR").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::remove_var("ZDOTDIR");
        }

        // First setup - with nerd font disabled
        let result = setup_zsh_integration(true, None);
        assert!(result.is_ok(), "Initial setup should succeed: {:?}", result);

        // First setup should not create a backup (no existing file)
        assert!(
            result.as_ref().unwrap().backup_path.is_none(),
            "Should not create backup on initial setup"
        );

        let content = fs::read_to_string(&zshrc_path).expect("Should be able to read zshrc");
        assert!(
            content.contains("export NERD_FONT=\"0\""),
            "Should contain NERD_FONT=0 after first setup"
        );
        assert!(
            !content.contains("export FORGE_EDITOR"),
            "Should not contain FORGE_EDITOR after first setup"
        );

        // Second setup - without nerd font but with editor
        let result = setup_zsh_integration(false, Some("nvim"));
        assert!(result.is_ok(), "Update setup should succeed: {:?}", result);

        // Second setup should create a backup (existing file)
        let backup_path = result.as_ref().unwrap().backup_path.as_ref();
        assert!(backup_path.is_some(), "Should create backup on update");
        let backup = backup_path.unwrap();
        assert!(backup.exists(), "Backup file should exist at {:?}", backup);

        // Verify backup filename contains timestamp
        let backup_name = backup.file_name().unwrap().to_str().unwrap();
        assert!(
            backup_name.starts_with(".zshrc.bak."),
            "Backup filename should start with .zshrc.bak.: {}",
            backup_name
        );
        assert!(
            backup_name.len() > ".zshrc.bak.".len(),
            "Backup filename should include timestamp: {}",
            backup_name
        );

        let content = fs::read_to_string(&zshrc_path).expect("Should be able to read zshrc");

        // Should not contain NERD_FONT=0 anymore
        assert!(
            !content.contains("export NERD_FONT=\"0\""),
            "Should not contain NERD_FONT=0 after update:\n{}",
            content
        );

        // Should contain the editor
        assert!(
            content.contains("export FORGE_EDITOR=\"nvim\""),
            "Should contain FORGE_EDITOR after update:\n{}",
            content
        );

        // Should still have markers
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));

        // Should only have one set of markers
        assert_eq!(
            content.matches("# >>> forge initialize >>>").count(),
            1,
            "Should have exactly one start marker"
        );
        assert_eq!(
            content.matches("# <<< forge initialize <<<").count(),
            1,
            "Should have exactly one end marker"
        );

        // Restore environment
        // SAFETY: We hold ENV_LOCK to prevent concurrent environment modifications
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(zdotdir) = original_zdotdir {
                std::env::set_var("ZDOTDIR", zdotdir);
            } else {
                std::env::remove_var("ZDOTDIR");
            }
        }
    }
}
