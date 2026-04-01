use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use clap::CommandFactory;
use clap_complete::generate;
use clap_complete::shells::Fish;
use include_dir::{Dir, include_dir};

use crate::cli::Cli;

/// Embeds fish plugin function files for fish integration
static FISH_PLUGIN_FUNCTIONS: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/../../fish-plugin/functions");

/// Embeds fish plugin conf.d files for fish integration
static FISH_PLUGIN_CONF: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/../../fish-plugin/conf.d");

/// Generates the complete fish plugin by combining embedded files and clap
/// completions
pub fn generate_fish_plugin() -> Result<String> {
    let mut output = String::new();

    // Emit conf.d files first (initialization, variables, bindings)
    for file in forge_embed::files(&FISH_PLUGIN_CONF) {
        let content = std::str::from_utf8(file.contents())?;
        for line in content.lines() {
            let trimmed = line.trim();
            // Skip empty lines and comment lines
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                output.push_str(line);
                output.push('\n');
            }
        }
    }

    // Emit function files (actions, helpers, dispatcher)
    for file in forge_embed::files(&FISH_PLUGIN_FUNCTIONS) {
        let content = std::str::from_utf8(file.contents())?;
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                output.push_str(line);
                output.push('\n');
            }
        }
    }

    // Generate clap completions for the CLI
    let mut cmd = Cli::command();
    let mut completions = Vec::new();
    generate(Fish, &mut cmd, "forge", &mut completions);

    // Append completions to the output with clear separator
    let completions_str = String::from_utf8(completions)?;
    output.push_str("\n# --- Clap Completions ---\n");
    output.push_str(&completions_str);

    // Set environment variable to indicate plugin is loaded (with timestamp)
    output.push_str("\nset -g _FORGE_PLUGIN_LOADED (date +%s)\n");

    Ok(output)
}

/// Generates the Fish theme for Forge
pub fn generate_fish_theme() -> Result<String> {
    let mut content = include_str!("../../../../fish-plugin/forge.theme.fish").to_string();

    // Set environment variable to indicate theme is loaded (with timestamp)
    content.push_str("\nset -g _FORGE_THEME_LOADED (date +%s)\n");

    Ok(content)
}

/// Executes a Fish script with streaming output
///
/// # Arguments
///
/// * `script_content` - The Fish script content to execute
/// * `script_name` - Descriptive name for the script (used in error messages)
///
/// # Errors
///
/// Returns error if the script cannot be executed, if output streaming fails,
/// or if the script exits with a non-zero status code
fn execute_fish_script_with_streaming(script_content: &str, script_name: &str) -> Result<()> {
    // Execute the script in a fish subprocess with piped output
    let mut child = std::process::Command::new("fish")
        .arg("-c")
        .arg(script_content)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(format!("Failed to execute fish {} script", script_name))?;

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
        .context(format!("Failed to wait for fish {} script", script_name))?;

    if !status.success() {
        let exit_code = status
            .code()
            .map_or_else(|| "unknown".to_string(), |code| code.to_string());

        anyhow::bail!(
            "Fish {} script failed with exit code: {}",
            script_name,
            exit_code
        );
    }

    Ok(())
}

/// Runs diagnostics on the Fish shell environment with streaming output
///
/// # Errors
///
/// Returns error if the doctor script cannot be executed
pub fn run_fish_doctor() -> Result<()> {
    let script_content = include_str!("../../../../fish-plugin/doctor.fish");
    execute_fish_script_with_streaming(script_content, "doctor")
}

/// Shows Fish keyboard shortcuts with streaming output
///
/// # Errors
///
/// Returns error if the keyboard script cannot be executed
pub fn run_fish_keyboard() -> Result<()> {
    let script_content = include_str!("../../../../fish-plugin/keyboard.fish");
    execute_fish_script_with_streaming(script_content, "keyboard")
}

/// Represents the state of markers in a file
enum MarkerState {
    /// No markers found
    NotFound,
    /// Valid markers with correct positions
    Valid { start: usize, end: usize },
    /// Invalid markers (incorrect order or incomplete)
    Invalid {
        start: Option<usize>,
        end: Option<usize>,
    },
}

/// Parses the file content to find and validate marker positions
fn parse_markers(lines: &[String], start_marker: &str, end_marker: &str) -> MarkerState {
    let start_idx = lines.iter().position(|line| line.trim() == start_marker);
    let end_idx = lines.iter().position(|line| line.trim() == end_marker);

    match (start_idx, end_idx) {
        (Some(start), Some(end)) if start < end => MarkerState::Valid { start, end },
        (None, None) => MarkerState::NotFound,
        (start, end) => MarkerState::Invalid { start, end },
    }
}

/// Result of Fish setup operation
#[derive(Debug)]
pub struct FishSetupResult {
    /// Status message describing what was done
    pub message: String,
    /// Path to backup file if one was created
    pub backup_path: Option<PathBuf>,
}

/// Sets up Fish integration with optional nerd font and editor configuration
///
/// # Arguments
///
/// * `disable_nerd_font` - If true, adds NERD_FONT=0 to config.fish
/// * `forge_editor` - If Some(editor), adds FORGE_EDITOR export to config.fish
///
/// # Errors
///
/// Returns error if:
/// - The HOME environment variable is not set
/// - The config.fish file cannot be read or written
/// - Invalid forge markers are found (incomplete or incorrectly ordered)
/// - A backup of the existing config.fish cannot be created
pub fn setup_fish_integration(
    disable_nerd_font: bool,
    forge_editor: Option<&str>,
) -> Result<FishSetupResult> {
    const START_MARKER: &str = "# >>> forge initialize >>>";
    const END_MARKER: &str = "# <<< forge initialize <<<";
    const FORGE_INIT_CONFIG: &str = include_str!("../../../../fish-plugin/forge.setup.fish");

    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let xdg_config =
        std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| format!("{}/.config", home));
    let config_dir = PathBuf::from(&xdg_config).join("fish");
    let config_path = config_dir.join("config.fish");

    // Ensure fish config directory exists
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)
            .context(format!("Failed to create {}", config_dir.display()))?;
    }

    // Read existing config.fish or create new one
    let content = if config_path.exists() {
        fs::read_to_string(&config_path)
            .context(format!("Failed to read {}", config_path.display()))?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    // Parse markers to determine their state
    let marker_state = parse_markers(&lines, START_MARKER, END_MARKER);

    // Build the forge config block with markers
    let mut forge_config: Vec<String> = vec![START_MARKER.to_string()];
    forge_config.extend(FORGE_INIT_CONFIG.lines().map(String::from));

    // Add nerd font configuration if requested
    if disable_nerd_font {
        forge_config.push(String::new());
        forge_config.push(
            "# Disable Nerd Fonts (set during setup - icons not displaying correctly)".to_string(),
        );
        forge_config.push("# To re-enable: remove this line and install a Nerd Font from https://www.nerdfonts.com/".to_string());
        forge_config.push("set -gx NERD_FONT 0".to_string());
    }

    // Add editor configuration if requested
    if let Some(editor) = forge_editor {
        forge_config.push(String::new());
        forge_config.push("# Editor for editing prompts (set during setup)".to_string());
        forge_config.push(
            "# To change: update FORGE_EDITOR or remove to use $EDITOR".to_string(),
        );
        forge_config.push(format!("set -gx FORGE_EDITOR \"{}\"", editor));
    }

    forge_config.push(END_MARKER.to_string());

    // Add or update forge configuration block based on marker state
    let (new_content, config_action) = match marker_state {
        MarkerState::Valid { start, end } => {
            // Markers exist - replace content between them
            lines.splice(start..=end, forge_config.iter().cloned());
            (lines.join("\n") + "\n", "updated")
        }
        MarkerState::Invalid { start, end } => {
            let location = match (start, end) {
                (Some(s), Some(e)) => {
                    Some(format!("{}:{}-{}", config_path.display(), s + 1, e + 1))
                }
                (Some(s), None) => Some(format!("{}:{}", config_path.display(), s + 1)),
                (None, Some(e)) => Some(format!("{}:{}", config_path.display(), e + 1)),
                (None, None) => None,
            };

            let mut error =
                anyhow::anyhow!("Invalid forge markers found in {}", config_path.display());
            if let Some(loc) = location {
                error = error.context(format!("Markers found at {}", loc));
            }
            return Err(error);
        }
        MarkerState::NotFound => {
            // No markers - add them at the end
            if !lines.is_empty() && !lines[lines.len() - 1].trim().is_empty() {
                lines.push(String::new());
            }

            lines.extend(forge_config.iter().cloned());
            (lines.join("\n") + "\n", "added")
        }
    };

    // Create backup of existing config.fish if it exists
    let backup_path = if config_path.exists() {
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");

        let parent = config_path
            .parent()
            .context("config path has no parent directory")?;
        let filename = config_path
            .file_name()
            .context("config path has no filename")?;
        let filename_str = filename
            .to_str()
            .context("config filename is not valid UTF-8")?;

        let backup = parent.join(format!("{}.bak.{}", filename_str, timestamp));
        fs::copy(&config_path, &backup)
            .context(format!("Failed to create backup at {}", backup.display()))?;
        Some(backup)
    } else {
        None
    };

    // Write back to config.fish
    fs::write(&config_path, &new_content)
        .context(format!("Failed to write to {}", config_path.display()))?;

    Ok(FishSetupResult {
        message: format!("forge plugins {}", config_action),
        backup_path,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{LazyLock, Mutex};

    use super::*;

    // Mutex to ensure tests that modify environment variables run serially
    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    /// Test that the doctor script executes and streams output
    #[test]
    fn test_run_fish_doctor_streaming() {
        // SAFETY: No mutex needed for single test
        unsafe {
            std::env::set_var("FORGE_SKIP_INTERACTIVE", "1");
        }

        let actual = run_fish_doctor();

        // Clean up
        // SAFETY: No mutex needed for single test
        unsafe {
            std::env::remove_var("FORGE_SKIP_INTERACTIVE");
        }

        // Accept success or expected failures (fish not installed in CI)
        match actual {
            Ok(_) => {}
            Err(e) => {
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
    fn test_setup_fish_integration_without_nerd_font_config() {
        use tempfile::TempDir;

        let _guard = ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".config").join("fish");

        let original_home = std::env::var("HOME").ok();
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();

        // SAFETY: We hold ENV_LOCK
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        let actual = setup_fish_integration(false, None);

        // Restore environment first
        // SAFETY: We hold ENV_LOCK
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(xdg) = original_xdg {
                std::env::set_var("XDG_CONFIG_HOME", xdg);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }

        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        let config_path = config_dir.join("config.fish");
        assert!(config_path.exists(), "config.fish should be created");
        let content = fs::read_to_string(&config_path).expect("Should be able to read config");

        assert!(!content.contains("NERD_FONT"));
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));
    }

    #[test]
    fn test_setup_fish_integration_with_nerd_font_disabled() {
        use tempfile::TempDir;

        let _guard = ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let original_home = std::env::var("HOME").ok();
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();

        // SAFETY: We hold ENV_LOCK
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        let actual = setup_fish_integration(true, None);
        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        let config_path = temp_dir
            .path()
            .join(".config")
            .join("fish")
            .join("config.fish");
        let content = fs::read_to_string(&config_path).expect("Should be able to read config");

        assert!(content.contains("set -gx NERD_FONT 0"));
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));

        // SAFETY: We hold ENV_LOCK
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            }
            if let Some(xdg) = original_xdg {
                std::env::set_var("XDG_CONFIG_HOME", xdg);
            }
        }
    }

    #[test]
    fn test_setup_fish_integration_with_editor() {
        use tempfile::TempDir;

        let _guard = ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let original_home = std::env::var("HOME").ok();
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();

        // SAFETY: We hold ENV_LOCK
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        let actual = setup_fish_integration(false, Some("code --wait"));
        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        let config_path = temp_dir
            .path()
            .join(".config")
            .join("fish")
            .join("config.fish");
        let content = fs::read_to_string(&config_path).expect("Should be able to read config");

        assert!(content.contains("set -gx FORGE_EDITOR \"code --wait\""));
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));

        // SAFETY: We hold ENV_LOCK
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(xdg) = original_xdg {
                std::env::set_var("XDG_CONFIG_HOME", xdg);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
    }

    #[test]
    fn test_setup_fish_integration_updates_existing_markers() {
        use tempfile::TempDir;

        let _guard = ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let original_home = std::env::var("HOME").ok();
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();

        // SAFETY: We hold ENV_LOCK
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        // First setup - with nerd font disabled
        let result = setup_fish_integration(true, None);
        assert!(result.is_ok(), "Initial setup should succeed: {:?}", result);
        assert!(
            result.as_ref().unwrap().backup_path.is_none(),
            "Should not create backup on initial setup"
        );

        let config_path = temp_dir
            .path()
            .join(".config")
            .join("fish")
            .join("config.fish");
        let content = fs::read_to_string(&config_path).expect("Should be able to read config");
        assert!(content.contains("set -gx NERD_FONT 0"));
        assert!(!content.contains("FORGE_EDITOR"));

        // Second setup - without nerd font but with editor
        let result = setup_fish_integration(false, Some("nvim"));
        assert!(result.is_ok(), "Update setup should succeed: {:?}", result);

        let backup_path = result.as_ref().unwrap().backup_path.as_ref();
        assert!(backup_path.is_some(), "Should create backup on update");
        let backup = backup_path.unwrap();
        assert!(backup.exists(), "Backup file should exist");

        // Verify backup filename contains timestamp
        let backup_name = backup.file_name().unwrap().to_str().unwrap();
        assert!(
            backup_name.starts_with("config.fish.bak."),
            "Backup filename should start with config.fish.bak.: {}",
            backup_name
        );
        assert!(
            backup_name.len() > "config.fish.bak.".len(),
            "Backup filename should include timestamp: {}",
            backup_name
        );

        let content = fs::read_to_string(&config_path).expect("Should be able to read config");
        assert!(!content.contains("NERD_FONT"));
        assert!(content.contains("set -gx FORGE_EDITOR \"nvim\""));
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));

        assert_eq!(content.matches("# >>> forge initialize >>>").count(), 1);
        assert_eq!(content.matches("# <<< forge initialize <<<").count(), 1);

        // SAFETY: We hold ENV_LOCK
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(xdg) = original_xdg {
                std::env::set_var("XDG_CONFIG_HOME", xdg);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
    }

    #[test]
    fn test_generate_fish_plugin() {
        let actual = generate_fish_plugin();
        assert!(actual.is_ok(), "generate_fish_plugin should succeed: {:?}", actual);

        let output = actual.unwrap();
        assert!(
            output.contains("_FORGE_PLUGIN_LOADED"),
            "Output should contain _FORGE_PLUGIN_LOADED"
        );
        assert!(
            output.contains("function _forge_accept_line"),
            "Output should contain function definitions"
        );
        assert!(
            output.contains("# --- Clap Completions ---"),
            "Output should contain clap completions separator"
        );
    }

    #[test]
    fn test_generate_fish_theme() {
        let actual = generate_fish_theme();
        assert!(actual.is_ok(), "generate_fish_theme should succeed: {:?}", actual);

        let output = actual.unwrap();
        assert!(
            output.contains("_FORGE_THEME_LOADED"),
            "Output should contain _FORGE_THEME_LOADED"
        );
        assert!(
            output.contains("fish_right_prompt") || output.contains("_forge_prompt_info"),
            "Output should contain fish_right_prompt or _forge_prompt_info"
        );
    }

    #[test]
    fn test_run_fish_keyboard_streaming() {
        // SAFETY: No mutex needed for single test
        unsafe {
            std::env::set_var("FORGE_SKIP_INTERACTIVE", "1");
        }

        let actual = run_fish_keyboard();

        // Clean up
        // SAFETY: No mutex needed for single test
        unsafe {
            std::env::remove_var("FORGE_SKIP_INTERACTIVE");
        }

        // Accept success or expected failures (fish not installed in CI)
        match actual {
            Ok(_) => {}
            Err(e) => {
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
    fn test_setup_fish_integration_with_both_configs() {
        use tempfile::TempDir;

        let _guard = ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let original_home = std::env::var("HOME").ok();
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();

        // SAFETY: We hold ENV_LOCK
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        // Run setup with both nerd font disabled and editor configured
        let actual = setup_fish_integration(true, Some("vim"));
        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        let config_path = temp_dir
            .path()
            .join(".config")
            .join("fish")
            .join("config.fish");
        let content = fs::read_to_string(&config_path).expect("Should be able to read config");

        // Should contain both configurations
        assert!(
            content.contains("set -gx NERD_FONT 0"),
            "Content should contain NERD_FONT 0:\n{}",
            content
        );
        assert!(
            content.contains("set -gx FORGE_EDITOR \"vim\""),
            "Content should contain FORGE_EDITOR:\n{}",
            content
        );

        // Should contain the markers
        assert!(content.contains("# >>> forge initialize >>>"));
        assert!(content.contains("# <<< forge initialize <<<"));

        // SAFETY: We hold ENV_LOCK
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(xdg) = original_xdg {
                std::env::set_var("XDG_CONFIG_HOME", xdg);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
    }

    #[test]
    fn test_setup_fish_integration_xdg_config_home() {
        use tempfile::TempDir;

        let _guard = ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let custom_xdg = temp_dir.path().join("custom_config");
        let original_home = std::env::var("HOME").ok();
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();

        // SAFETY: We hold ENV_LOCK
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("XDG_CONFIG_HOME", &custom_xdg);
        }

        let actual = setup_fish_integration(false, None);
        assert!(actual.is_ok(), "Setup should succeed: {:?}", actual);

        // config.fish should be created under XDG_CONFIG_HOME, not under HOME/.config
        let expected_path = custom_xdg.join("fish").join("config.fish");
        assert!(
            expected_path.exists(),
            "config.fish should be created at {:?}",
            expected_path
        );

        let home_default_path = temp_dir
            .path()
            .join(".config")
            .join("fish")
            .join("config.fish");
        assert!(
            !home_default_path.exists(),
            "config.fish should NOT be created at default HOME path {:?}",
            home_default_path
        );

        // SAFETY: We hold ENV_LOCK
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(xdg) = original_xdg {
                std::env::set_var("XDG_CONFIG_HOME", xdg);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
    }
}
