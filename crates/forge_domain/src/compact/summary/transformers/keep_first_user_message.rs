use super::super::SummaryMessage;
use crate::compact::summary::ContextSummary;
use crate::{Role, Transformer};

/// Keeps only the first user message in consecutive user message sequences.
///
/// This transformer processes a context summary and filters out consecutive
/// user messages, keeping only the first one in each sequence. Messages with
/// other roles (System, Assistant) are preserved as-is.
pub struct KeepFirstUserMessage;

impl Transformer for KeepFirstUserMessage {
    type Value = ContextSummary;

    fn transform(&mut self, summary: Self::Value) -> Self::Value {
        let mut messages: Vec<SummaryMessage> = Vec::new();
        let mut last_role = Role::System;
        for message in summary.messages {
            let role = message.role;
            if role == Role::User {
                if last_role != Role::User {
                    messages.push(message)
                }
            } else {
                messages.push(message)
            }

            last_role = role;
        }

        ContextSummary { messages }
    }
}
