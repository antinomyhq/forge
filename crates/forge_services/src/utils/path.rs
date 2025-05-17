use std::path::Path;

use anyhow::bail;

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

/// Formats a path for display, converting absolute paths to relative when
/// possible
///
/// If the path starts with the current working directory, returns a
/// relative path. Otherwise, returns the original absolute path.
///
/// # Arguments
/// * `path` - The path to format
/// * `cwd` - The current working directory path
///
/// # Returns
/// * `Ok(String)` with a formatted path string
pub fn format_display_path(path: &Path, cwd: &Path) -> anyhow::Result<String> {
    // Try to create a relative path for display if possible
    let display_path = if path.starts_with(cwd) {
        match path.strip_prefix(cwd) {
            Ok(rel_path) => rel_path.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    } else {
        path.display().to_string()
    };

    if display_path.is_empty() {
        Ok(".".to_string())
    } else {
        Ok(display_path)
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
    fn test_format_display_path_relative() {
        let cwd = Path::new("/home/user/projects");
        let path = Path::new("/home/user/projects/subfolder/file.txt");

        let result = format_display_path(path, cwd);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "subfolder/file.txt");
    }

    #[test]
    fn test_format_display_path_absolute() {
        let cwd = Path::new("/home/user/projects");
        let path = Path::new("/var/log/file.log");

        let result = format_display_path(path, cwd);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/var/log/file.log");
    }

    #[test]
    fn test_parent_dir_relative_path() {
        let path = Path::new("../parent/path");
        assert!(assert_absolute_path(path).is_err());
    }

    #[test]
    fn test_cwd() {
        let cwd = Path::new("/home/user/projects");
        let path = Path::new("/home/user/projects");

        let result = format_display_path(path, cwd);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ".");
    }
}
