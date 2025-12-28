use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use forge_app::{FileWriterInfra, FsMultiPatchService, MultiPatchOutput, compute_hash};
use forge_domain::{FSMultiPatch, SnapshotRepository, ValidationRepository};
use tokio::fs;

use crate::tool_services::fs_patch::apply_replacement;
use crate::tool_services::PatchError;
use crate::utils::assert_absolute_path;

/// Service for applying multiple patch operations in sequence to a single file
///
/// This service applies multiple edits sequentially, where each edit operates on the
/// result of the previous edit. This is useful for making multiple related
/// changes to a file without having to read and patch multiple times.
pub struct ForgeFsMultiPatch<F> {
    infra: Arc<F>,
}

impl<F> ForgeFsMultiPatch<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<F: FileWriterInfra + SnapshotRepository + ValidationRepository>
    FsMultiPatchService for ForgeFsMultiPatch<F>
{
    async fn multi_patch(
        &self,
        patches: FSMultiPatch,
    ) -> anyhow::Result<MultiPatchOutput> {
        let path = Path::new(&patches.path);
        assert_absolute_path(path)?;

        if patches.edits.is_empty() {
            return Err(anyhow::anyhow!("No edits provided for multi-patch operation"));
        }

        // Read the original content once
        let mut current_content = fs::read_to_string(path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
        let old_content = current_content.clone();

        // Apply each edit sequentially
        for (index, edit) in patches.edits.iter().enumerate() {
            current_content = apply_replacement(
                current_content,
                edit.search.clone(),
                &edit.operation,
                &edit.content,
            )
            .map_err(|e: PatchError| {
                anyhow::anyhow!("Failed to apply edit #{}: {}", index + 1, e)
            })?;
        }

        // SNAPSHOT COORDINATION: Always capture snapshot before modifying
        self.infra.insert_snapshot(path).await?;

        // Write final content to file
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

        Ok(MultiPatchOutput {
            errors,
            before: old_content,
            after: current_content,
            content_hash,
            edits_applied: patches.edits.len(),
            results: vec![],
        })
    }
}
