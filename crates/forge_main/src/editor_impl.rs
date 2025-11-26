    /// Handle editor command
    async fn on_editor(&mut self) -> Result<()> {
        use std::env;
        use tempfile::NamedTempFile;

        let editor = env::var("FORGE_EDITOR")
            .or_else(|_| env::var("EDITOR"))
            .unwrap_or_else(|_| "nano".to_string());

        let temp_file = NamedTempFile::with_suffix(".md")?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        match self.api.execute_editor_command(&format!("{} {}", editor, temp_path)).await {
            Ok(status) if status.success() => {
                match std::fs::read_to_string(&temp_path) {
                    Ok(content) => {
                        if !content.trim().is_empty() {
                            self.console.set_buffer(content);
                        }
                    }
                    Err(e) => {
                        self.writeln(format!("Failed to read editor content: {}", e))?;
                    }
                }
            }
            Err(e) => {
                self.writeln(format!("Failed to launch editor: {}", e))?;
            }
        }
    }