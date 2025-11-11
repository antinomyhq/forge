use std::path::{Path, PathBuf};

use anyhow::Result;
use forge_app::DirectoryReaderInfra;
use forge_fs::ForgeFS;
use futures::future::join_all;
use glob::Pattern;

/// Service for reading multiple files from a directory asynchronously
pub struct ForgeDirectoryReaderService;

impl ForgeDirectoryReaderService {
    /// Reads all files in a directory that match the given filter pattern
    /// Returns a vector of tuples containing (file_path, file_content)
    /// Files are read asynchronously/in parallel for better performance
    async fn read_directory_files(
        &self,
        directory: &Path,
        pattern: Option<&str>,
    ) -> Result<Vec<(PathBuf, String)>> {
        // Check if directory exists
        if !ForgeFS::exists(directory) || ForgeFS::is_file(directory) {
            return Ok(vec![]);
        }

        // Build glob pattern if filter is provided
        let glob_pattern = if let Some(pattern) = pattern {
            Some(Pattern::new(pattern)?)
        } else {
            None
        };

        // Read directory entries
        let mut dir = ForgeFS::read_dir(directory).await?;
        let mut file_paths = Vec::new();

        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();

            // Only process files (not directories)
            if ForgeFS::is_file(&path) {
                // Apply filter if provided
                if let Some(ref pattern) = glob_pattern {
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
                        && pattern.matches(file_name)
                    {
                        file_paths.push(path);
                    }
                } else {
                    file_paths.push(path);
                }
            }
        }

        // Read all files in parallel
        let read_tasks = file_paths.into_iter().map(|path| {
            let path_clone = path.clone();
            async move {
                match ForgeFS::read_to_string(&path).await {
                    Ok(content) => Some((path_clone, content)),
                    Err(_) => None, // Skip files that can't be read
                }
            }
        });

        let results = join_all(read_tasks).await;

        // Collect successful reads
        let files = results.into_iter().flatten().collect();

        Ok(files)
    }
}

#[async_trait::async_trait]
impl DirectoryReaderInfra for ForgeDirectoryReaderService {
    async fn read_directory_files(
        &self,
        directory: &Path,
        pattern: Option<&str>,
    ) -> Result<Vec<(PathBuf, String)>> {
        self.read_directory_files(directory, pattern).await
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::*;

    fn write_file(path: &Path, content: &str) {
        fs::write(path, content).unwrap();
    }

    #[tokio::test]
    async fn test_read_directory_files_with_filter() {
        let fixture = tempdir().unwrap();
        write_file(&fixture.path().join("test.md"), "# Markdown content");
        write_file(&fixture.path().join("test.txt"), "Text content");
        write_file(&fixture.path().join("test.rs"), "fn main() {}");

        let actual = ForgeDirectoryReaderService
            .read_directory_files(fixture.path(), Some("*.md"))
            .await
            .unwrap();

        let expected = vec![(
            fixture.path().join("test.md"),
            "# Markdown content".to_string(),
        )];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_directory_files_without_filter() {
        let fixture = tempdir().unwrap();
        write_file(&fixture.path().join("file1.txt"), "Content 1");
        write_file(&fixture.path().join("file2.md"), "Content 2");

        let mut actual = ForgeDirectoryReaderService
            .read_directory_files(fixture.path(), None)
            .await
            .unwrap();
        actual.sort_by(|(a, _), (b, _)| a.file_name().cmp(&b.file_name()));

        let expected = vec![
            (fixture.path().join("file1.txt"), "Content 1".to_string()),
            (fixture.path().join("file2.md"), "Content 2".to_string()),
        ];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_directory_files_nonexistent_directory() {
        let actual = ForgeDirectoryReaderService
            .read_directory_files(Path::new("/nonexistent"), None)
            .await
            .unwrap();

        let expected: Vec<(PathBuf, String)> = vec![];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_directory_files_ignores_subdirectories() {
        let fixture = tempdir().unwrap();
        write_file(&fixture.path().join("test.txt"), "File content");

        let subdir = fixture.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        write_file(&subdir.join("subfile.txt"), "Sub content");

        let actual = ForgeDirectoryReaderService
            .read_directory_files(fixture.path(), None)
            .await
            .unwrap();

        let expected = vec![(fixture.path().join("test.txt"), "File content".to_string())];
        assert_eq!(actual, expected);
    }
}
