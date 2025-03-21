use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use forge_walker::Walker;
use tracing::{error, warn};

use crate::shard::Shard;
use crate::token_estimator::TokenEstimator;

/// Represents a file with its content and estimated token count.
#[derive(Debug)]
struct FileContent {
    path: PathBuf,
    content: String,
    estimated_tokens: usize,
}

/// Merges all non-binary files in a directory into a single string or multiple
/// shards. Each file's content is preceded by its full path with a separator.
pub struct Merger {
    root_dir: PathBuf,
    separator: String,
    token_limit: Option<usize>,
    token_estimator: TokenEstimator,
}

impl Merger {
    /// Create a new Merger instance
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
            separator: "================".to_string(),
            token_limit: None,
            token_estimator: TokenEstimator::default(),
        }
    }

    /// Set a custom separator for file headers
    pub fn with_separator<S: Into<String>>(mut self, separator: S) -> Self {
        self.separator = separator.into();
        self
    }

    /// Set a token limit for sharded processing
    pub fn with_token_limit(mut self, token_limit: usize) -> Self {
        self.token_limit = Some(token_limit);
        self
    }

    /// Set a custom token estimator
    pub fn with_token_estimator(mut self, estimator: TokenEstimator) -> Self {
        self.token_estimator = estimator;
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
                        error!("Error reading {}: {}", full_path.display(), e);
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

    /// Process files with token-based sharding
    pub async fn process_sharded(&self) -> Result<Vec<String>> {
        // Default token limit if not specified
        let token_limit = self.token_limit.unwrap_or(8000);

        // Collect file contents with token estimates
        let file_contents = self.collect_file_contents().await?;

        // Create shards
        let shards = self.create_shards(file_contents, token_limit);

        // Extract string content from shards
        let shard_contents: Vec<String> = shards.into_iter().map(|shard| shard.content).collect();

        Ok(shard_contents)
    }

    /// Process files with token-based sharding, returning detailed shard
    /// information
    pub async fn process_sharded_with_info(&self) -> Result<Vec<Shard>> {
        // Default token limit if not specified
        let token_limit = self.token_limit.unwrap_or(8000);

        // Collect file contents with token estimates
        let file_contents = self.collect_file_contents().await?;

        // Create shards
        let shards = self.create_shards(file_contents, token_limit);

        Ok(shards)
    }

    // Helper method to collect file contents with token estimates
    async fn collect_file_contents(&self) -> Result<Vec<FileContent>> {
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

        // Prepare to collect all file contents with token estimates
        let mut file_contents = Vec::new();
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
                        error!("Error reading {}: {}", full_path.display(), e);
                    }
                    continue;
                }
            };

            // Format the file content with header
            let formatted_content = format!(
                "{0}\nFile: {1}\n{0}\n{2}",
                self.separator,
                full_path.display(),
                content
            );

            // Estimate token count
            let estimated_tokens = self.token_estimator.estimate(&formatted_content);

            file_contents.push(FileContent {
                path: full_path,
                content: formatted_content,
                estimated_tokens,
            });
        }

        Ok(file_contents)
    }

    // Helper method to create shards from file contents
    fn create_shards(&self, mut file_contents: Vec<FileContent>, token_limit: usize) -> Vec<Shard> {
        let mut shards = Vec::new();

        // Sort files by token count (largest first) to improve packing efficiency
        file_contents.sort_by(|a, b| b.estimated_tokens.cmp(&a.estimated_tokens));

        // Track current shard state
        let mut current_content = String::new();
        let mut current_tokens = 0;
        let mut current_file_paths = Vec::new();

        for file in file_contents {
            // If this file alone exceeds the token limit, place it in its own shard with a
            // warning
            if file.estimated_tokens > token_limit {
                let warning = format!(
                    "\nWARNING: File exceeds token limit by {} tokens\n",
                    file.estimated_tokens - token_limit
                );

                let content_with_warning = warning + &file.content;

                shards.push(Shard {
                    content: content_with_warning,
                    estimated_tokens: file.estimated_tokens,
                    file_paths: vec![file.path.clone()],
                });

                // Log warning using tracing instead of eprintln
                warn!(
                    file_path = %file.path.display(),
                    file_tokens = file.estimated_tokens,
                    token_limit = token_limit,
                    "File exceeds token limit"
                );

                continue;
            }

            // If adding this file would exceed the limit and we already have content,
            // finalize the current shard and start a new one
            if current_tokens + file.estimated_tokens > token_limit && !current_content.is_empty() {
                // Finalize current shard
                shards.push(Shard {
                    content: current_content,
                    estimated_tokens: current_tokens,
                    file_paths: current_file_paths,
                });

                // Start a new shard
                current_content = file.content;
                current_tokens = file.estimated_tokens;
                current_file_paths = vec![file.path];
            } else {
                // Add to current shard
                if !current_content.is_empty() {
                    current_content.push('\n');
                }
                current_content.push_str(&file.content);
                current_tokens += file.estimated_tokens;
                current_file_paths.push(file.path);
            }
        }

        // Add the final shard if not empty
        if !current_content.is_empty() {
            shards.push(Shard {
                content: current_content,
                estimated_tokens: current_tokens,
                file_paths: current_file_paths,
            });
        }

        shards
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

    // New tests for token-based sharding

    #[tokio::test]
    async fn test_sharding_basic() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();

        // Create test files
        let file1_path = temp_path.join("file1.txt");
        let file2_path = temp_path.join("file2.txt");
        let file3_path = temp_path.join("file3.txt");

        // We'll create content that would roughly translate to specific token counts
        // Using the 0.75 * word count estimation
        let file1_content = "This is file one. It has enough words to make it around 10 tokens with our estimation method.";
        let file2_content = "File two content. Also designed to be about 10 tokens.";
        let file3_content =
            "File three has some content that should be estimated at around 10 tokens too.";

        fs::write(&file1_path, file1_content).await?;
        fs::write(&file2_path, file2_content).await?;
        fs::write(&file3_path, file3_content).await?;

        // Create merger with a very small token limit to force sharding (around 25
        // tokens) This should put file1 and file2 in one shard, and file3 in
        // another
        let merger = Merger::new(temp_path).with_token_limit(25);

        // Test the basic process_sharded method
        let shards = merger.process_sharded().await?;

        // Should be split into at least 2 shards
        assert!(
            shards.len() >= 2,
            "Expected at least 2 shards, got {}",
            shards.len()
        );

        // Each shard should contain file content
        let mut found_file1 = false;
        let mut found_file2 = false;
        let mut found_file3 = false;

        for shard in &shards {
            if shard.contains(&format!("File: {}", file1_path.display())) {
                found_file1 = true;
            }
            if shard.contains(&format!("File: {}", file2_path.display())) {
                found_file2 = true;
            }
            if shard.contains(&format!("File: {}", file3_path.display())) {
                found_file3 = true;
            }
        }

        assert!(found_file1, "File 1 not found in any shard");
        assert!(found_file2, "File 2 not found in any shard");
        assert!(found_file3, "File 3 not found in any shard");

        // Test the detailed method
        let detailed_shards = merger.process_sharded_with_info().await?;

        // Verify each shard has estimated tokens and file paths
        for shard in &detailed_shards {
            assert!(
                shard.estimated_tokens > 0,
                "Shard should have token estimate"
            );
            assert!(!shard.file_paths.is_empty(), "Shard should have file paths");
            assert!(!shard.content.is_empty(), "Shard should have content");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_large_file_handling() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();

        // Create a large file that will exceed the token limit
        let large_file_path = temp_path.join("large.txt");
        let small_file_path = temp_path.join("small.txt");

        // Create a string with many words to exceed our tiny token limit
        let large_content = "This file has many words and will exceed our token limit. ".repeat(20);
        let small_content = "Small file content.";

        fs::write(&large_file_path, &large_content).await?;
        fs::write(&small_file_path, small_content).await?;

        // Create merger with a small token limit
        let merger = Merger::new(temp_path).with_token_limit(30);

        // Get the detailed shards
        let shards = merger.process_sharded_with_info().await?;

        // Verify the large file is in its own shard with a warning
        let large_file_shards: Vec<_> = shards
            .iter()
            .filter(|s| s.file_paths.contains(&large_file_path))
            .collect();

        assert_eq!(
            large_file_shards.len(),
            1,
            "Large file should be in exactly one shard"
        );
        let large_shard = &large_file_shards[0];

        // The large file should be the only one in its shard
        assert_eq!(
            large_shard.file_paths.len(),
            1,
            "Large file should be alone in its shard"
        );

        // The content should contain a warning
        assert!(
            large_shard
                .content
                .contains("WARNING: File exceeds token limit by"),
            "Large file shard should contain a warning"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_different_token_limits() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();

        // Create several files with content
        for i in 1..=10 {
            let file_path = temp_path.join(format!("file{}.txt", i));
            let content = format!("This is file {}. It has some content.", i).repeat(5);
            fs::write(&file_path, content).await?;
        }

        // Test with different token limits
        let limits = [50, 100, 200, 400];
        let mut shard_counts = Vec::new();

        for limit in limits {
            let merger = Merger::new(temp_path).with_token_limit(limit);
            let shards = merger.process_sharded().await?;
            shard_counts.push(shards.len());
        }

        // Higher limits should produce fewer or equal number of shards
        for i in 1..limits.len() {
            assert!(
                shard_counts[i] <= shard_counts[i - 1],
                "Higher token limit should produce fewer or equal number of shards"
            );
        }

        Ok(())
    }
}
