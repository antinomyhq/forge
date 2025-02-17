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

use std::ops::Range;

use crate::{Context, ContextMessage, Role};

pub struct Summarize<'context> {
    context: &'context mut Context,
    token_limit: usize,
    // TODO: use a persistent cache to avoid re-summarizing
}

impl<'context> Summarize<'context> {
    pub fn new(context: &'context mut Context, token_limit: usize) -> Self {
        Self { context, token_limit }
    }

    fn replace(&mut self, content: impl ToString, range: Range<usize>) {
        // TODO: improve the quality of summary message
        let content = format!("\n<work_summary>\n{}\n</work_summary>", content.to_string());
        let message = ContextMessage::assistant(content, None);
        // replace the 1st ocurrence with summarized message.
        self.context.messages[range.start] = message;

        // remove all the messages between the range.
        self.context
            .messages
            .splice(range.start + 1..range.end, std::iter::empty());
    }

    /// Get a replaceable item while the total token count is above the limit
    pub fn summarize(&mut self) -> Option<Summary<'_, 'context>> {
        let total = token_count(&self.context.to_text());

        if total <= self.token_limit {
            return None;
        }

        // get the next range that can be summarized.
        next_turn(self.context).and_then(|turn| {
            if turn.end >= self.context.messages.len() {
                // if it's the on-going user message, then don't summarize it.
                // we only summarize the completed user messages.
                None
            } else {
                Some(Summary { summarize: self, next_turn: turn })
            }
        })
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

/// Finds the first turn of a user message in the given context.
///
/// Returns a range of indices representing the first user message
/// up to (but not including) the next user message. If there's no
/// second user message, the range extends to the end of messages.
fn next_turn(context: &Context) -> Option<Range<usize>> {
    let start = context
        .messages
        .iter()
        .position(|m| m.has_role(Role::User))?;
    let end = context
        .messages
        .iter()
        .skip(start + 1)
        .position(|m| m.has_role(Role::User));

    Some(start..end.map_or(context.messages.len(), |e| start + 1 + e))
}

#[cfg(test)]
mod tests {
    use std::fmt::Display;

    use super::*;

    #[derive(Default, Debug)]
    struct SummarizeSpec {
        description: String,
        input: Context,
        output: Context,
    }

    impl SummarizeSpec {
        fn summarize(input: &mut Context, token_limit: usize, summary_text: &str) -> Context {
            let mut summarizer = Summarize::new(input, token_limit);
            let mut summary = summarizer.summarize();
            if let Some(summarize) = &mut summary {
                summarize.set(summary_text.to_string());
            }
            summarizer.context.clone()
        }

        fn new(description: &str, mut input: Context, summary: &str, token_limit: usize) -> Self {
            let input_contxt = input.clone();
            let output_context = Self::summarize(&mut input, token_limit, summary);
            Self {
                description: description.to_string(),
                input: input_contxt,
                output: output_context,
            }
        }
    }

    impl Display for Context {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for message in self.messages.iter() {
                match message {
                    ContextMessage::ContentMessage(content_message) => match content_message.role {
                        Role::User => writeln!(f, "User: {}", content_message.content)?,
                        Role::Assistant => writeln!(f, "Assistant: {}", content_message.content)?,
                        Role::System => writeln!(f, "System: {}", content_message.content)?,
                    },
                    ContextMessage::ToolMessage(tool_message) => {
                        writeln!(
                            f,
                            "Tool:\n name:{}\ncontent:{}",
                            tool_message.name.as_str(),
                            tool_message.content
                        )?;
                    }
                }
            }

            Ok(())
        }
    }

    impl Display for SummarizeSpec {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "\n[{}]", self.description)?;
            writeln!(f, "[Before]")?;
            writeln!(f, "{}", self.input)?;
            writeln!(f, "[After]")?;
            if self.input != self.output {
                writeln!(f, "{}", self.output)?;
            } else {
                writeln!(f, "No changes has been made to context")?;
            }
            Ok(())
        }
    }

    #[derive(Default, Debug)]
    struct SummarizeSuit(Vec<SummarizeSpec>);

    impl SummarizeSuit {
        fn add(
            &mut self,
            description: &str,
            input: Vec<ContextMessage>,
            summary: &str,
            token: usize,
        ) {
            self.0.push(SummarizeSpec::new(
                description,
                Context::default().messages(input),
                summary,
                token,
            ));
        }
    }

    impl Display for SummarizeSuit {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for spec in &self.0 {
                writeln!(f, "{}", spec)?;
            }
            Ok(())
        }
    }

    #[test]
    fn test_combined_summarize_suite() {
        let mut suite = SummarizeSuit::default();

        suite.add(
            "Shouldn't summarize when user message isn't completed.",
            vec![
                ContextMessage::user("Short message".to_string()),
                ContextMessage::assistant("Brief response".to_string(), None),
            ],
            "Summary of the user message",
            10,
        );

        suite.add(
            "Shouldn't summarize when there's no user message",
            vec![
                ContextMessage::system("System message".to_string()),
                ContextMessage::assistant("Assistant message".to_string(), None),
                ContextMessage::assistant("Another assistant message".to_string(), None),
            ],
            "Summary of the user message",
            100,
        );

        suite.add(
            "Summarize the completed user message",
            vec![
                ContextMessage::system("System Prompt".to_string()),
                ContextMessage::user("User Question".to_string()),
                ContextMessage::assistant("Answer-1".to_string(), None),
                ContextMessage::assistant("Answer-2".to_string(), None),
                ContextMessage::assistant("Answer-3".to_string(), None),
                ContextMessage::assistant("Answer-4".to_string(), None),
                ContextMessage::user("okay, what are tools avail to you?".to_string()),
                ContextMessage::assistant(
                    "i have only one tool avail to me and it's `shell` tool.".to_string(),
                    None,
                ),
            ],
            "User asked some question, which agent answered.",
            15,
        );

        suite.add(
            "Should not summarize when context token count is less then specified",
            vec![
                ContextMessage::system("System Prompt".to_string()),
                ContextMessage::user("User Question".to_string()),
                ContextMessage::assistant("Answer-1".to_string(), None),
                ContextMessage::assistant("Answer-2".to_string(), None),
                ContextMessage::assistant("Answer-3".to_string(), None),
                ContextMessage::assistant("Answer-4".to_string(), None),
                ContextMessage::user("okay, what are tools avail to you?".to_string()),
                ContextMessage::assistant(
                    "i have only one tool avail to me and it's `shell` tool.".to_string(),
                    None,
                ),
            ],
            "User asked some question, which agent answered.",
            1000,
        );

        insta::assert_snapshot!(suite);
    }
}
