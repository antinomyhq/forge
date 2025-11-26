use std::path::Path;
use std::sync::Arc;
use std::{env, fs};

use anyhow::{Result, bail};
use forge_app::{CommandInfra, EnvironmentInfra};
use tempfile::NamedTempFile;

/// Service for handling external editor operations
pub struct ForgeEditor<I> {
    infra: Arc<I>,
}

/// Simple editor service for tests
pub struct EditorService;

impl Default for EditorService {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorService {
    pub fn new() -> Self {
        Self
    }

    /// Get editor command from environment variables
    pub fn get_editor_command(&self) -> String {
        env::var("FORGE_EDITOR")
            .or_else(|_| env::var("EDITOR"))
            .unwrap_or_else(|_| "nano".to_string())
    }

    /// Create temporary markdown file
    pub fn create_temp_file(&self) -> Result<NamedTempFile> {
        Ok(NamedTempFile::with_suffix(".md")?)
    }

    /// Read file content synchronously
    pub fn read_file_content(&self, path: &Path) -> Result<String> {
        Ok(fs::read_to_string(path)?)
    }

    /// Check if command is an editor command
    pub fn is_editor_command(command: &str) -> bool {
        const EDITORS: &[&str] = &[
            "nano",
            "vim",
            "vi",
            "emacs",
            "code",
            "subl",
            "atom",
            "notepad++",
            "notepad",
            "gedit",
            "kate",
            "mousepad",
            "leafpad",
            "xed",
            "micro",
        ];

        let command_lower = command.to_lowercase();
        EDITORS
            .iter()
            .any(|&pattern| command_lower.contains(pattern) || command_lower.starts_with(pattern))
    }

    /// Check if shell is restricted
    pub fn is_restricted_shell(shell: &str) -> bool {
        shell.contains("rbash")
    }

    /// Open editor and return content
    pub async fn open_editor(&self) -> Result<String> {
        let editor = self.get_editor_command();

        if editor.is_empty() {
            bail!("FORGE_EDITOR environment variable not set");
        }

        let temp_file = self.create_temp_file()?;
        let temp_path = temp_file.path();

        // Check if we're in restricted mode
        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        if Self::is_restricted_shell(&shell) && !Self::is_editor_command(&editor) {
            eprintln!("Note: Using external editor (restricted mode detected)");
            eprintln!("Note: Some editors may have limited functionality in restricted mode");
            eprintln!("Note: Set FORGE_EDITOR environment variable if needed");
        }

        // Launch editor
        let mut cmd = tokio::process::Command::new(&editor);
        cmd.arg(temp_path);
        let output = cmd.output().await?;

        if !output.status.success() {
            bail!(
                "Editor '{}' exited with error: {}",
                editor,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Read content after editor closes
        let content = self.read_file_content(temp_path)?;

        // Temp file will be automatically deleted when temp_file goes out of scope
        Ok(content)
    }
}

impl<I> ForgeEditor<I>
where
    I: CommandInfra + EnvironmentInfra,
{
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    /// Open editor and return content
    pub async fn open_editor(&self) -> Result<String> {
        let editor = self.infra.get_editor_command();

        if editor.is_empty() {
            bail!("FORGE_EDITOR environment variable not set");
        }

        let temp_file = self.create_temp_file()?;
        let temp_path = temp_file.path();

        // Check if we're in restricted mode
        let shell = self.infra.get_shell();
        let is_restricted_mode = self.infra.is_restricted() || Self::is_restricted_shell(&shell);
        if is_restricted_mode && !Self::is_editor_command(&editor) {
            eprintln!("Note: Using external editor (restricted mode detected)");
            eprintln!("Note: Some editors may have limited functionality in restricted mode");
            eprintln!("Note: Set FORGE_EDITOR environment variable if needed");
        }

        // Launch editor using command infrastructure
        let output = self
            .infra
            .execute_command_with_args(&editor, &[temp_path.to_str().unwrap()])
            .await?;

        if output.exit_code != Some(0) {
            bail!("Editor '{}' exited with error: {}", editor, output.stderr);
        }

        // Read content after editor closes
        let content = fs::read_to_string(temp_path)?;

        // Temp file will be automatically deleted when temp_file goes out of scope
        Ok(content)
    }

    /// Create temporary markdown file
    fn create_temp_file(&self) -> Result<NamedTempFile> {
        Ok(NamedTempFile::with_suffix(".md")?)
    }

    /// Check if command is an editor command
    fn is_editor_command(command: &str) -> bool {
        EditorService::is_editor_command(command)
    }

    /// Check if shell is restricted
    fn is_restricted_shell(shell: &str) -> bool {
        EditorService::is_restricted_shell(shell)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_command_detection() {
        assert!(EditorService::is_editor_command("nano"));
        assert!(EditorService::is_editor_command("vim"));
        assert!(EditorService::is_editor_command("code"));
        assert!(!EditorService::is_editor_command("ls"));
        assert!(!EditorService::is_editor_command("cat"));
    }

    #[test]
    fn test_restricted_shell_detection() {
        assert!(EditorService::is_restricted_shell("/bin/rbash"));
        assert!(EditorService::is_restricted_shell("/usr/bin/rbash"));
        assert!(!EditorService::is_restricted_shell("/bin/bash"));
        assert!(!EditorService::is_restricted_shell("/bin/zsh"));
    }

    #[test]
    fn test_get_editor_command_default() {
        unsafe {
            std::env::remove_var("FORGE_EDITOR");
            std::env::remove_var("EDITOR");
        }

        let service = EditorService::new();
        let command = service.get_editor_command();
        assert_eq!(command, "nano");
    }

    #[test]
    fn test_get_editor_command_precedence() {
        let service = EditorService::new();

        unsafe {
            std::env::set_var("FORGE_EDITOR", "vim");
            std::env::set_var("EDITOR", "nano");
        }
        let command = service.get_editor_command();
        assert_eq!(command, "vim");

        unsafe {
            std::env::remove_var("FORGE_EDITOR");
        }
        let command = service.get_editor_command();
        assert_eq!(command, "nano");

        unsafe {
            std::env::remove_var("EDITOR");
        }
    }

    #[test]
    fn test_restricted_mode_logic() {
        // Test that restricted mode is correctly detected from both flag and shell

        // When shell is normal but restricted flag is true -> should check editor list
        let shell_normal = "/bin/bash";
        let restricted_flag_true = true;
        let is_restricted_mode_flag = restricted_flag_true || EditorService::is_restricted_shell(shell_normal);
        assert!(is_restricted_mode_flag);

        // When shell is rbash but restricted flag is false -> should check editor list
        let shell_rbash = "/bin/rbash";
        let restricted_flag_false = false;
        let is_restricted_mode_shell = restricted_flag_false || EditorService::is_restricted_shell(shell_rbash);
        assert!(is_restricted_mode_shell);

        // When both are normal -> should not check editor list
        let is_restricted_mode_none = restricted_flag_false || EditorService::is_restricted_shell(shell_normal);
        assert!(!is_restricted_mode_none);
    }
}
