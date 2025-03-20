//! Forge Merger crate
//!
//! This crate provides functionality to merge all non-ignored files
//! in a directory into a single string.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use forge_walker::Walker;

/// Merges all non-binary files in a directory into a single string.
/// Each file's content is preceded by its full path with a separator.
pub struct Merger {
    root_dir: PathBuf,
    separator: String,
}

impl Merger {
    /// Create a new Merger instance
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
            separator: "================".to_string(),
        }
    }

    /// Set a custom separator for file headers
    pub fn with_separator<S: Into<String>>(mut self, separator: S) -> Self {
        self.separator = separator.into();
        self
    }

    /// Process all files and merge their content into a string
    pub async fn process(&self) -> Result<String> {
        // Ensure the root directory exists
        if !self.root_dir.exists() {
            return Err(anyhow::anyhow!(
                "Directory '{}' does not exist",
                self.root_dir.display()
            ));
        }

        // Use Walker to get all files
        let walker = Walker::max_all().cwd(self.root_dir.clone());

        let files = walker
            .get()
            .await
            .with_context(|| format!("Failed to walk directory '{}'", self.root_dir.display()))?;
        // Prepare to collect all file contents
        let mut merged_content = String::new();
        let mut seen_paths = HashSet::new();

        for file in files {
            if file.is_dir() {
                continue;
            }

            let path = Path::new(&file.path);
            let full_path = self.root_dir.join(path);

            // Skip if we've already processed this file
            if !seen_paths.insert(full_path.clone()) {
                continue;
            }

            // Try to read the file content
            let content = match tokio::fs::read_to_string(&full_path).await {
                Ok(content) => content,
                Err(e) => {
                    // Skip binary or unreadable files silently
                    if e.kind() != std::io::ErrorKind::InvalidData {
                        eprintln!("Error reading {:?}: {}", full_path.display(), e);
                    }
                    continue;
                }
            };

            // Add file header with full path
            if !merged_content.is_empty() {
                merged_content.push('\n');
            }

            // Enclose the file path with separators
            merged_content.push_str(&format!(
                "{0}\nFile: {1}\n{0}\n",
                self.separator,
                full_path.display()
            ));
            merged_content.push_str(&content);
        }

        // Return the merged content
        Ok(merged_content)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;

    use tokio::fs;

    use super::*;

    #[tokio::test]
    async fn test_merger() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();

        // Create a few test files
        let file1_path = temp_path.join("file1.txt");
        let file2_path = temp_path.join("file2.txt");
        let output_path = temp_path.join("merged.txt");

        fs::write(&file1_path, "Content of file 1").await?;
        fs::write(&file2_path, "Content of file 2").await?;

        // Create and run the merger
        let merger = Merger::new(temp_path);
        let merged_content = merger.process().await?;

        // Write the merged content to verify it
        fs::write(&output_path, &merged_content).await?;

        // Verify the output
        let mut output_content = String::new();
        File::open(&output_path)?.read_to_string(&mut output_content)?;

        // Check that both file paths and contents are in the output
        assert!(output_content.contains(&format!("File: {}", file1_path.display())));
        assert!(output_content.contains(&format!("File: {}", file2_path.display())));
        assert!(output_content.contains("Content of file 1"));
        assert!(output_content.contains("Content of file 2"));
        assert!(output_content.contains("================"));

        // Verify the new format with separators surrounding the file path
        assert!(output_content.contains("================\nFile:"));

        // Also verify the content directly from the process method
        assert!(merged_content.contains(&format!("File: {}", file1_path.display())));
        assert!(merged_content.contains(&format!("File: {}", file2_path.display())));
        assert!(merged_content.contains("Content of file 1"));
        assert!(merged_content.contains("Content of file 2"));

        Ok(())
    }

    #[tokio::test]
    async fn test_merger_with_custom_separator() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();

        // Create a test file
        let file_path = temp_path.join("test.txt");
        let output_path = temp_path.join("merged.txt");

        fs::write(&file_path, "Test content").await?;

        // Create and run the merger with a custom separator
        let merger = Merger::new(temp_path).with_separator("---CUSTOM---");
        let merged_content = merger.process().await?;

        // Write the merged content to verify it
        fs::write(&output_path, &merged_content).await?;

        // Verify the output
        let mut output_content = String::new();
        File::open(&output_path)?.read_to_string(&mut output_content)?;

        assert!(output_content.contains(&format!("File: {}", file_path.display())));
        assert!(output_content.contains("Test content"));
        assert!(output_content.contains("---CUSTOM---"));
        assert!(!output_content.contains("================"));

        // Verify the new format with custom separators surrounding the file path
        assert!(output_content.contains("---CUSTOM---\nFile:"));

        // Also verify the content directly from the process method
        assert!(merged_content.contains(&format!("File: {}", file_path.display())));
        assert!(merged_content.contains("Test content"));
        assert!(merged_content.contains("---CUSTOM---"));

        Ok(())
    }

    #[tokio::test]
    async fn test_merger_with_subdirectories() -> Result<()> {
        // Create a temporary directory with subdirectories
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();
        let subdir_path = temp_path.join("subdir");
        fs::create_dir(&subdir_path).await?;

        // Create files in both main directory and subdirectory
        let file1_path = temp_path.join("root.txt");
        let file2_path = subdir_path.join("nested.txt");
        let output_path = temp_path.join("merged.txt");

        fs::write(&file1_path, "Root file").await?;
        fs::write(&file2_path, "Nested file").await?;

        // Create and run the merger
        let merger = Merger::new(temp_path);
        let merged_content = merger.process().await?;

        // Write the merged content to verify it
        fs::write(&output_path, &merged_content).await?;

        // Verify the output
        let mut output_content = String::new();
        File::open(&output_path)?.read_to_string(&mut output_content)?;

        assert!(output_content.contains(&format!("File: {}", file1_path.display())));
        assert!(output_content.contains(&format!("File: {}", file2_path.display())));
        assert!(output_content.contains("Root file"));
        assert!(output_content.contains("Nested file"));

        // Verify the new format with separators surrounding the file path
        assert!(output_content.contains("================\nFile:"));

        // Also verify the content directly from the process method
        assert!(merged_content.contains(&format!("File: {}", file1_path.display())));
        assert!(merged_content.contains("Root file"));
        assert!(merged_content.contains("Nested file"));

        Ok(())
    }

    #[tokio::test]
    async fn test_exact_format() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();

        // Create a test file
        let file_path = temp_path.join("test.txt");
        let output_path = temp_path.join("merged.txt");

        fs::write(&file_path, "File content").await?;

        // Create and run the merger
        let merger = Merger::new(temp_path);
        let merged_content = merger.process().await?;

        // Write the merged content to verify it
        fs::write(&output_path, &merged_content).await?;

        // Verify the output
        let mut output_content = String::new();
        File::open(&output_path)?.read_to_string(&mut output_content)?;

        // Check the exact format
        let expected_format = format!(
            "================\nFile: {}\n================\nFile content",
            file_path.display()
        );
        assert!(output_content.contains(&expected_format));

        // Also verify the content directly from the process method
        assert!(merged_content.contains(&expected_format));

        Ok(())
    }
}
