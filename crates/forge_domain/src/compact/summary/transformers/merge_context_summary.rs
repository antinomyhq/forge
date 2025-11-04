use super::MergeSummaryMessage;
use crate::Transformer;
use crate::compact::summary::ContextSummary;

/// Merges all messages within a context summary.
///
/// This transformer applies the `MergeSummaryMessage` transformer to each
/// message in the context summary, consolidating mergeable message blocks
/// across the entire context.
pub struct MergeContextSummary;

impl Transformer for MergeContextSummary {
    type Value = ContextSummary;

    fn transform(&mut self, mut summary: Self::Value) -> Self::Value {
        for message in summary.messages.iter_mut() {
            let transformed = MergeSummaryMessage.transform(message.clone());
            *message = transformed;
        }

        summary
    }
}
