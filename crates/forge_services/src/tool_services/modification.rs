use std::path::Path;

use crate::{FileReaderInfra, SnapshotInfra};

/// Service for detecting external modifications to files by comparing current
/// content with snapshots.
pub struct ForgeModificationService<F>(std::sync::Arc<F>);

impl<F> ForgeModificationService<F> {
    pub fn new(infra: std::sync::Arc<F>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + SnapshotInfra> forge_app::ModificationService
    for ForgeModificationService<F>
{
    async fn detect(&self, path: &Path) -> anyhow::Result<bool> {
        // Read current file content as bytes
        let current_content = self.0.read(path).await?;

        // Retrieve latest snapshot content for modification detection
        let snapshot_content = self.0.get_latest_snapshot(path).await?;

        // Determine if file has been modified externally
        Ok(has_external_modification(
            &current_content,
            snapshot_content.as_deref(),
        ))
    }
}

/// Determines if a file has been modified externally by comparing current
/// content with snapshot
///
/// # Arguments
/// * `current` - Current file content as bytes
/// * `snapshot` - Optional snapshot content as bytes
///
/// # Returns
/// * `false` if snapshot is None (no snapshot = no external modification)
/// * `true` if snapshot exists and differs from current content
/// * `false` if snapshot exists and matches current content
fn has_external_modification(current: &[u8], snapshot: Option<&[u8]>) -> bool {
    match snapshot {
        None => false,
        Some(snap) => current != snap,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_has_external_modification_none_snapshot() {
        let current = b"Hello, World!";
        let snapshot = None;

        let actual = has_external_modification(current, snapshot);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_identical_content() {
        let current = b"Hello, World!";
        let snapshot = Some(b"Hello, World!".as_slice());

        let actual = has_external_modification(current, snapshot);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_different_content() {
        let current = b"Hello, World!";
        let snapshot = Some(b"Goodbye, World!".as_slice());

        let actual = has_external_modification(current, snapshot);
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_empty_files() {
        let current = b"";
        let snapshot = Some(b"".as_slice());

        let actual = has_external_modification(current, snapshot);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_unicode_content() {
        let current = "🚀 Hello, World! 🌍".as_bytes();
        let snapshot = Some("🚀 Hello, World! 🌍".as_bytes());

        let actual = has_external_modification(current, snapshot);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_unicode_different() {
        let current = "🚀 Hello, World! 🌍".as_bytes();
        let snapshot = Some("🌍 Goodbye, World! 🚀".as_bytes());

        let actual = has_external_modification(current, snapshot);
        let expected = true;

        assert_eq!(actual, expected);
    }
}
