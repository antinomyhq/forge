use std::convert::identity;

use crate::{SummaryMessageBlock, SummaryToolCall};

/// Trait for types that can determine if they can be merged with another
/// instance
pub trait CanMerge {
    /// Checks if this instance can be merged with another instance
    fn can_merge(&self, other: &Self) -> bool;
}

impl<T: CanMerge> CanMerge for Vec<T> {
    fn can_merge(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .zip(other)
                .all(|(this, that)| this.can_merge(that))
    }
}

impl CanMerge for SummaryMessageBlock {
    fn can_merge(&self, other: &Self) -> bool {
        [
            self.content == other.content,
            self.tool_call_success == other.tool_call_success,
            self.tool_call.can_merge(&other.tool_call),
        ]
        .into_iter()
        .all(identity)
    }
}

impl CanMerge for SummaryToolCall {
    fn can_merge(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Mcp { name: a }, Self::Mcp { name: b }) if a == b => true,
            (Self::FileRead { path: a }, Self::FileRead { path: b }) if a == b => true,
            (Self::FileUpdate { path: a }, Self::FileUpdate { path: b }) if a == b => true,
            (Self::FileRemove { path: a }, Self::FileRemove { path: b }) if a == b => true,
            (Self::Execute { cmd: a }, Self::Execute { cmd: b }) if a == b => true,
            (Self::Fetch { url: a }, Self::Fetch { url: b }) if a == b => true,
            _ => false,
        }
    }
}
