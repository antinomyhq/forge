use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use forge_app::{FsPatchService, PatchOutput};
use forge_domain::PatchOperation;
use thiserror::Error;
use tokio::fs;

// No longer using dissimilar for fuzzy matching
use crate::utils::assert_absolute_path;
use crate::{FsWriteService, Infrastructure};

// Removed fuzzy matching threshold as we only use exact matching now

/// A match found in the source text. Represents a range in the source text that
/// can be used for extraction or replacement operations. Stores the position
/// and length to allow efficient substring operations.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
struct Range {
    /// Starting position of the match in source text
    start: usize,
    /// Length of the matched text
    length: usize,
}

impl Range {
    /// Create a new match from a start position and length
    fn new(start: usize, length: usize) -> Self {
        Self { start, length }
    }

    /// Get the end position (exclusive) of this match
    fn end(&self) -> usize {
        self.start + self.length
    }

    /// Try to find an exact match in the source text
    fn find_exact(source: &str, search: &str) -> Option<Self> {
        source
            .find(search)
            .map(|start| Self::new(start, search.len()))
    }

    // Fuzzy matching removed - we only use exact matching
}

impl From<Range> for std::ops::Range<usize> {
    fn from(m: Range) -> Self {
        m.start..m.end()
    }
}

// MatchSequence struct and implementation removed - we only use exact matching

#[derive(Debug, Error)]
enum Error {
    #[error("Failed to read/write file: {0}")]
    FileOperation(#[from] std::io::Error),
    #[error("Could not find match for search text: {0}")]
    NoMatch(String),
    #[error("Could not find swap target text: {0}")]
    NoSwapTarget(String),
}

fn apply_replacement(
    source: String,
    search: &str,
    operation: &PatchOperation,
    content: &str,
) -> Result<String, Error> {
    // Handle empty search string - only certain operations make sense here
    if search.is_empty() {
        return match operation {
            // Append to the end of the file
            PatchOperation::Append => Ok(format!("{source}{content}")),
            // Prepend to the beginning of the file
            PatchOperation::Prepend => Ok(format!("{content}{source}")),
            // Replace is equivalent to completely replacing the file
            PatchOperation::Replace => Ok(content.to_string()),
            // Swap doesn't make sense with empty search - keep source unchanged
            PatchOperation::Swap => Ok(source),
        };
    }

    // Find the exact match to operate on
    let patch =
        Range::find_exact(&source, search).ok_or_else(|| Error::NoMatch(search.to_string()))?;

    // Apply the operation based on its type
    match operation {
        // Prepend content before the matched text
        PatchOperation::Prepend => Ok(format!(
            "{}{}{}",
            &source[..patch.start],
            content,
            &source[patch.start..]
        )),

        // Append content after the matched text
        PatchOperation::Append => Ok(format!(
            "{}{}{}",
            &source[..patch.end()],
            content,
            &source[patch.end()..]
        )),

        // Replace matched text with new content
        PatchOperation::Replace => Ok(format!(
            "{}{}{}",
            &source[..patch.start],
            content,
            &source[patch.end()..]
        )),

        // Swap with another text in the source
        PatchOperation::Swap => {
            // Find the target text to swap with
            let target_patch = Range::find_exact(&source, content)
                .ok_or_else(|| Error::NoSwapTarget(content.to_string()))?;

            // Handle the case where patches overlap
            if (patch.start <= target_patch.start && patch.end() > target_patch.start)
                || (target_patch.start <= patch.start && target_patch.end() > patch.start)
            {
                // For overlapping ranges, we just do an ordinary replacement
                return Ok(format!(
                    "{}{}{}",
                    &source[..patch.start],
                    content,
                    &source[patch.end()..]
                ));
            }

            // We need to handle different ordering of patches
            if patch.start < target_patch.start {
                // Original text comes first
                Ok(format!(
                    "{}{}{}{}{}",
                    &source[..patch.start],
                    content,
                    &source[patch.end()..target_patch.start],
                    &source[patch.start..patch.end()],
                    &source[target_patch.end()..]
                ))
            } else {
                // Target text comes first
                Ok(format!(
                    "{}{}{}{}{}",
                    &source[..target_patch.start],
                    &source[patch.start..patch.end()],
                    &source[target_patch.end()..patch.start],
                    content,
                    &source[patch.end()..]
                ))
            }
        }
    }
}

// Using PatchOperation from forge_domain

// Using FSPatchInput from forge_domain

/// Modifies files with targeted text operations on matched patterns. Supports
/// prepend, append, replace, swap, delete operations on first pattern
/// occurrence. Ideal for precise changes to configs, code, or docs while
/// preserving context. Not suitable for complex refactoring or modifying all
/// pattern occurrences - use forge_tool_fs_create instead for complete
/// rewrites and forge_tool_fs_undo for undoing the last operation. Fails if
/// search pattern isn't found.
pub struct ForgeFsPatch<F>(Arc<F>);

impl<F: Infrastructure> ForgeFsPatch<F> {
    pub fn new(input: Arc<F>) -> Self {
        Self(input)
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsPatchService for ForgeFsPatch<F> {
    async fn patch(
        &self,
        input_path: String,
        search: String,
        operation: PatchOperation,
        content: String,
    ) -> anyhow::Result<PatchOutput> {
        let path = Path::new(&input_path);
        assert_absolute_path(path)?;
        // Read the original content once
        // TODO: use forge_fs
        let mut current_content = fs::read_to_string(path)
            .await
            .map_err(Error::FileOperation)?;
        // Save the old content before modification for diff generation
        let old_content = current_content.clone();
        // Apply the replacement
        current_content = apply_replacement(current_content, &search, &operation, &content)?;

        // Write final content to file after all patches are applied
        self.0
            .file_write_service()
            .write(path, Bytes::from(current_content.clone()), true)
            .await?;

        Ok(PatchOutput {
            warning: super::syn::validate(path, &current_content).map(|e| e.to_string()),
            before: old_content,
            after: current_content,
        })
    }
}
