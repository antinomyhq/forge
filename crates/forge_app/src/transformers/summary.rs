use std::path::PathBuf;

use forge_domain::{ContextSummary, Role, Transformer};

use crate::transformers::dedupe_role::DedupeRole;
use crate::transformers::drop_role::DropRole;
use crate::transformers::strip_working_dir::StripWorkingDir;
use crate::transformers::trim_context_summary::TrimContextSummary;

/// Composes all compaction transformers into a single transformation pipeline.
///
/// This transformer applies a series of transformations to reduce context size
/// and improve context quality:
///
/// 1. Drops all System role messages
/// 2. Deduplicates consecutive User role messages
/// 3. Trims context by keeping only the last operation per file path
/// 4. Deduplicates consecutive Assistant content blocks
/// 5. Strips working directory prefix from file paths
///
/// The transformations are applied in sequence using the pipe combinator.
pub struct SummaryTransformer {
    working_dir: PathBuf,
}

impl SummaryTransformer {
    /// Creates a new Compaction transformer with the specified working
    /// directory.
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The working directory path to strip from file paths
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self { working_dir: working_dir.into() }
    }
}

impl Transformer for SummaryTransformer {
    type Value = ContextSummary;

    fn transform(&mut self, context_summary: Self::Value) -> Self::Value {
        DropRole::new(Role::System)
            .pipe(DedupeRole::new(Role::User))
            .pipe(TrimContextSummary)
            .pipe(StripWorkingDir::new(self.working_dir.clone()))
            .transform(context_summary)
    }
}
