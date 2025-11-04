use crate::compact::summary::{SummaryMessage, SummaryMessageBlock};
use crate::{CanMerge, Transformer};

/// Merges consecutive summary message blocks that can be merged together.
///
/// This transformer processes a `SummaryMessage` and consolidates adjacent
/// `SummaryMessageBlock` instances that are mergeable according to the
/// `CanMerge` trait implementation.
pub struct MergeSummaryMessage;

impl Transformer for MergeSummaryMessage {
    type Value = SummaryMessage;

    fn transform(&mut self, summary: Self::Value) -> Self::Value {
        let mut messages: Vec<SummaryMessageBlock> = Vec::new();

        for message in summary.messages {
            if let Some(last) = messages.last_mut()
                && last.can_merge(&message)
            {
                *last = message;
            } else {
                messages.push(message);
            }
        }

        SummaryMessage { role: summary.role, messages }
    }
}
