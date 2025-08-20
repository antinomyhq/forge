use std::path::{Path, PathBuf};

use anyhow::bail;

pub async fn get_git_root(from_dir: &Path) -> Option<PathBuf> {
    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(from_dir)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|root| PathBuf::from(root.trim()))
    } else {
        None
    }
}

/// Ensures that the given path is absolute
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// * `Ok(())` if the path is absolute
/// * `Err(String)` with an error message if the path is relative
pub fn assert_absolute_path(path: &Path) -> anyhow::Result<()> {
    if !path.is_absolute() {
        bail!("Path must be absolute. Please provide an absolute path starting with '/' (Unix) or 'C:\\' (Windows)".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unix_absolute_path() {
        let path = Path::new("/absolute/path");
        assert!(assert_absolute_path(path).is_ok());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_absolute_path() {
        let path = Path::new("C:\\Windows\\Path");
        assert!(assert_absolute_path(path).is_ok());
    }

    #[test]
    fn test_basic_relative_path() {
        let path = Path::new("relative/path");
        assert!(assert_absolute_path(path).is_err());
    }

    #[test]
    fn test_current_dir_relative_path() {
        let path = Path::new("./current/path");
        assert!(assert_absolute_path(path).is_err());
    }

    #[test]
    fn test_parent_dir_relative_path() {
        let path = Path::new("../parent/path");
        assert!(assert_absolute_path(path).is_err());
    }
}
