//! Context Summarization:
//! - Break the conversation into "turns"
//! - A turn is a sequence of messages where the first message is a user message
//! - Summarization happens for each turn independently with the oldest turn
//!   getting the highest priority.
//! - Summarization is done by removing all assistant/tool messages within a
//!   turn and replacing it with a summary as an assistant message.
//! - If a turn summary isn't enough to hit the thresholds, then the next turn
//!   is summarized.
//! - If after summarization of all the turns the threshold is still not met,
//!   then all the turns have to summarized again (summary of summary)
//! - NOTE: User and System messages are never summarized

use std::collections::VecDeque;
use std::ops::Range;

use crate::{Context, ContextMessage, Role};

pub struct Summarize<'context> {
    context: &'context mut Context,
    token_limit: usize,
    turns: VecDeque<Range<usize>>,
    // TODO: use a persistent cache to avoid re-summarizing
}

impl<'context> Summarize<'context> {
    pub fn new(context: &'context mut Context, token_limit: usize) -> Self {
        let turns = turns(context);
        Self { context, token_limit, turns: turns.into() }
    }

    fn replace(&mut self, content: impl ToString, range: Range<usize>) {
        // TODO: improve the quality of summary message
        let content = format!("\n<work_summary>\n{}\n</work_summary>", content.to_string());
        let message = ContextMessage::assistant(content, None);
        self.context.messages[range].fill(message);
    }

    /// Get a replaceable item while the total token count is above the limit
    pub fn summarize(&mut self) -> Option<Summary<'_, 'context>> {
        let total = token_count(&self.context.to_text());

        if total <= self.token_limit {
            return None;
        }

        self.turns
            .pop_front()
            .map(|turn| Summary { summarize: self, next_turn: turn })
    }
}

pub struct Summary<'this, 'context> {
    summarize: &'this mut Summarize<'context>,
    next_turn: Range<usize>,
}

impl Summary<'_, '_> {
    pub fn set(&mut self, message: impl ToString) {
        self.summarize.replace(message, self.next_turn.clone());
    }

    pub fn get(&self) -> String {
        Context::default()
            .messages(self.summarize.context.messages[self.next_turn.clone()].to_vec())
            .to_text()
    }
}

// TODO: this is a quick hack to get a ballpark token count
fn token_count(text: &str) -> usize {
    text.split_whitespace().count() * 75 / 100
}

fn turns(context: &Context) -> Vec<Range<usize>> {
    let starts = context
        .messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.has_role(Role::User))
        .map(|(i, _)| i);

    let ends = starts
        .clone()
        .skip(1)
        .chain(std::iter::once(context.messages.len()))
        .map(|i| i - 1);

    starts
        .zip(ends)
        .map(|(start, end)| start..end)
        .collect::<Vec<_>>()
}
