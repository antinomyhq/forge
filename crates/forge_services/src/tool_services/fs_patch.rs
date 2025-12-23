use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use forge_app::domain::PatchOperation;
use forge_app::{FileWriterInfra, FsPatchService, PatchOutput, compute_hash};
use forge_domain::{SnapshotRepository, ValidationRepository};
use thiserror::Error;
use tokio::fs;

use crate::utils::assert_absolute_path;

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
    #[error(
        "Could not find match for search text: '{0}'. File may have changed externally, consider reading the file again."
    )]
    NoMatch(String),
    #[error("Could not find swap target text: {0}")]
    NoSwapTarget(String),
    #[error(
        "Multiple matches found for search text: '{0}'. Either provide a more specific search pattern or use replace_all to replace all occurrences."
    )]
    MultipleMatches(String),
}

fn apply_replacement(
    haystack: String,
    search: Option<String>,
    operation: &PatchOperation,
    content: &str,
) -> Result<String, Error> {
    // Handle empty search string - only certain operations make sense here
    if let Some(needle) = search.and_then(|needle| {
        if needle.is_empty() {
            None // Empty search is not valid for matching
        } else {
            Some(needle)
        }
    }) {
        // Find the exact match to operate on
        let patch: Range = Range::find_exact(&haystack, needle.as_str())
            .ok_or_else(|| Error::NoMatch(needle.to_string()))?;

        // Apply the operation based on its type
        match operation {
            // Prepend content before the matched text
            PatchOperation::Prepend => Ok(format!(
                "{}{}{}",
                &haystack[..patch.start],
                content,
                &haystack[patch.start..]
            )),

            // Replace all occurrences of the matched text with new content
            PatchOperation::ReplaceAll => Ok(haystack.replace(needle.as_str(), content)),

            // Append content after the matched text
            PatchOperation::Append => Ok(format!(
                "{}\n{}{}",
                &haystack[..patch.end()],
                content,
                &haystack[patch.end()..]
            )),

            // Replace matched text with new content
            PatchOperation::Replace => {
                // Check if there are multiple matches
                let mut match_count = 0;
                let mut search_start = 0;
                while let Some(pos) = haystack[search_start..].find(needle.as_str()) {
                    match_count += 1;
                    if match_count > 1 {
                        return Err(Error::MultipleMatches(needle.to_string()));
                    }
                    search_start += pos + needle.len();
                }

                Ok(format!(
                    "{}{}{}",
                    &haystack[..patch.start],
                    content,
                    &haystack[patch.end()..]
                ))
            }

            // Swap with another text in the source
            PatchOperation::Swap => {
                // Find the target text to swap with
                let target_patch = Range::find_exact(&haystack, content)
                    .ok_or_else(|| Error::NoSwapTarget(content.to_string()))?;

                // Handle the case where patches overlap
                if (patch.start <= target_patch.start && patch.end() > target_patch.start)
                    || (target_patch.start <= patch.start && target_patch.end() > patch.start)
                {
                    // For overlapping ranges, we just do an ordinary replacement
                    return Ok(format!(
                        "{}{}{}",
                        &haystack[..patch.start],
                        content,
                        &haystack[patch.end()..]
                    ));
                }

                // We need to handle different ordering of patches
                if patch.start < target_patch.start {
                    // Original text comes first
                    Ok(format!(
                        "{}{}{}{}{}",
                        &haystack[..patch.start],
                        content,
                        &haystack[patch.end()..target_patch.start],
                        &haystack[patch.start..patch.end()],
                        &haystack[target_patch.end()..]
                    ))
                } else {
                    // Target text comes first
                    Ok(format!(
                        "{}{}{}{}{}",
                        &haystack[..target_patch.start],
                        &haystack[patch.start..patch.end()],
                        &haystack[target_patch.end()..patch.start],
                        content,
                        &haystack[patch.end()..]
                    ))
                }
            }
        }
    } else {
        match operation {
            // Append to the end of the file
            PatchOperation::Append => Ok(format!("{haystack}\n{content}")),
            // Prepend to the beginning of the file
            PatchOperation::Prepend => Ok(format!("{content}{haystack}")),
            // Replace is equivalent to completely replacing the file
            PatchOperation::Replace | PatchOperation::ReplaceAll => Ok(content.to_string()),
            // Swap doesn't make sense with empty search - keep source unchanged
            PatchOperation::Swap => Ok(haystack),
        }
    }
}

// Using PatchOperation from forge_domain

// Using FSPatchInput from forge_domain

/// Service for patching files with snapshot coordination
///
/// This service coordinates between infrastructure (file I/O) and repository
/// (snapshots) to modify files while preserving the ability to undo changes.
pub struct ForgeFsPatch<F> {
    infra: Arc<F>,
}

impl<F> ForgeFsPatch<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<F: FileWriterInfra + SnapshotRepository + ValidationRepository> FsPatchService
    for ForgeFsPatch<F>
{
    async fn patch(
        &self,
        input_path: String,
        search: Option<String>,
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
        current_content = apply_replacement(current_content, search, &operation, &content)?;

        // SNAPSHOT COORDINATION: Always capture snapshot before modifying
        self.infra.insert_snapshot(path).await?;

        // Write final content to file after all patches are applied
        self.infra
            .write(path, Bytes::from(current_content.clone()))
            .await?;

        // Compute hash of the final file content
        let content_hash = compute_hash(&current_content);

        // Validate file syntax using remote validation API (graceful failure)
        let errors = self
            .infra
            .validate_file(path, &current_content)
            .await
            .unwrap_or_default();

        Ok(PatchOutput {
            errors,
            before: old_content,
            after: current_content,
            content_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use forge_app::domain::PatchOperation;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_apply_replacement_replace_multiple_matches_error() {
        let source = "test test test";
        let search = Some("test".to_string());
        let operation = PatchOperation::Replace;
        let content = "replaced";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Multiple matches found for search text: 'test'. Either provide a more specific search pattern or use replace_all to replace all occurrences."));
    }

    #[test]
    fn test_apply_replacement_replace_single_match_success() {
        let source = "hello world test";
        let search = Some("world".to_string());
        let operation = PatchOperation::Replace;
        let content = "universe";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "hello universe test");
    }

    #[test]
    fn test_apply_replacement_prepend() {
        let source = "b\nc\nd";
        let search = Some("b".to_string());
        let operation = PatchOperation::Prepend;
        let content = "a\n".to_string();

        let result = super::apply_replacement(source.to_string(), search, &operation, &content);
        assert_eq!(result.unwrap(), "a\nb\nc\nd");
    }

    #[test]
    fn test_apply_replacement_prepend_empty() {
        let source = "b\nc\nd";
        let search = Some("".to_string());
        let operation = PatchOperation::Prepend;
        let content = "a\n".to_string();

        let result = super::apply_replacement(source.to_string(), search, &operation, &content);
        assert_eq!(result.unwrap(), "a\nb\nc\nd");
    }

    #[test]
    fn test_apply_replacement_prepend_no_search() {
        let source = "hello world";
        let search = None;
        let operation = PatchOperation::Prepend;
        let content = "prefix ";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "prefix hello world");
    }

    #[test]
    fn test_apply_replacement_append() {
        let source = "hello world";
        let search = Some("hello".to_string());
        let operation = PatchOperation::Append;
        let content = " there";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "hello\n there world");
    }

    #[test]
    fn test_apply_replacement_append_no_search() {
        let source = "hello world";
        let search = None;
        let operation = PatchOperation::Append;
        let content = " suffix";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "hello world\n suffix");
    }

    #[test]
    fn test_apply_replacement_replace() {
        let source = "hello world";
        let search = Some("world".to_string());
        let operation = PatchOperation::Replace;
        let content = "universe";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "hello universe");
    }

    #[test]
    fn test_apply_replacement_replace_no_search() {
        let source = "hello world";
        let search = None;
        let operation = PatchOperation::Replace;
        let content = "new content";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "new content");
    }

    #[test]
    fn test_apply_replacement_swap() {
        let source = "apple banana cherry";
        let search = Some("apple".to_string());
        let operation = PatchOperation::Swap;
        let content = "banana";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "banana apple cherry");
    }

    #[test]
    fn test_apply_replacement_swap_reverse_order() {
        let source = "apple banana cherry";
        let search = Some("banana".to_string());
        let operation = PatchOperation::Swap;
        let content = "apple";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "banana apple cherry");
    }

    #[test]
    fn test_apply_replacement_swap_overlapping() {
        let source = "abcdef";
        let search = Some("abc".to_string());
        let operation = PatchOperation::Swap;
        let content = "cde";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "cdedef");
    }

    #[test]
    fn test_apply_replacement_swap_no_search() {
        let source = "hello world";
        let search = None;
        let operation = PatchOperation::Swap;
        let content = "anything";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_apply_replacement_multiline() {
        let source = "line1\nline2\nline3";
        let search = Some("line2".to_string());
        let operation = PatchOperation::Replace;
        let content = "replaced_line";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "line1\nreplaced_line\nline3");
    }

    #[test]
    fn test_apply_replacement_with_special_chars() {
        let source = "hello $world @test";
        let search = Some("$world".to_string());
        let operation = PatchOperation::Replace;
        let content = "$universe";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "hello $universe @test");
    }

    #[test]
    fn test_apply_replacement_empty_content() {
        let source = "hello world test";
        let search = Some("world ".to_string());
        let operation = PatchOperation::Replace;
        let content = "";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "hello test");
    }

    #[test]
    fn test_apply_replacement_first_occurrence_only() {
        let source = "test test test";
        let search = Some("test".to_string());
        let operation = PatchOperation::Replace;
        let content = "replaced";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Multiple matches found for search text: 'test'")
        );
    }

    // Error cases
    #[test]
    fn test_apply_replacement_no_match() {
        let source = "hello world";
        let search = Some("missing".to_string());
        let operation = PatchOperation::Replace;
        let content = "replacement";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Could not find match for search text: 'missing'")
        );
    }

    #[test]
    fn test_apply_replacement_swap_no_target() {
        let source = "hello world";
        let search = Some("hello".to_string());
        let operation = PatchOperation::Swap;
        let content = "missing";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Could not find swap target text: missing")
        );
    }

    #[test]
    fn test_apply_replacement_edge_case_same_text() {
        let source = "hello hello";
        let search = Some("hello".to_string());
        let operation = PatchOperation::Swap;
        let content = "hello";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "hello hello");
    }

    #[test]
    fn test_apply_replacement_whitespace_handling() {
        let source = "  hello   world  ";
        let search = Some("hello   world".to_string());
        let operation = PatchOperation::Replace;
        let content = "test";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "  test  ");
    }

    #[test]
    fn test_apply_replacement_unicode() {
        let source = "héllo wørld 🌍";
        let search = Some("wørld".to_string());
        let operation = PatchOperation::Replace;
        let content = "univérse";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "héllo univérse 🌍");
    }

    #[test]
    fn test_apply_replacement_replace_all_multiple_occurrences() {
        let source = "test test test";
        let search = Some("test".to_string());
        let operation = PatchOperation::ReplaceAll;
        let content = "replaced";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "replaced replaced replaced");
    }

    #[test]
    fn test_apply_replacement_replace_all_no_search() {
        let source = "hello world";
        let search = None;
        let operation = PatchOperation::ReplaceAll;
        let content = "new content";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "new content");
    }

    #[test]
    fn test_apply_replacement_replace_all_empty_search() {
        let source = "hello world";
        let search = Some("".to_string());
        let operation = PatchOperation::ReplaceAll;
        let content = "new content";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert_eq!(result.unwrap(), "new content");
    }

    #[test]
    fn test_apply_replacement_replace_all_no_match() {
        let source = "hello world";
        let search = Some("missing".to_string());
        let operation = PatchOperation::ReplaceAll;
        let content = "replacement";

        let result = super::apply_replacement(source.to_string(), search, &operation, content);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Could not find match for search text: 'missing'")
        );
    }
}
