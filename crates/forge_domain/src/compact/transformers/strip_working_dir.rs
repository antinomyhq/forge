use std::path::Path;

use crate::Transformer;
use crate::compact::summary::{ContextSummary, SummaryToolCall};

/// Strips the working directory prefix from all file paths in tool calls.
///
/// This transformer removes the working directory prefix from file paths in
/// FileRead, FileUpdate, and FileRemove tool calls, making paths relative to
/// the working directory. This is useful for reducing context size and making
/// summaries more portable across different environments.
pub struct StripWorkingDir {
    working_dir: String,
}

impl StripWorkingDir {
    /// Creates a new StripWorkingDir transformer with the specified working
    /// directory.
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The working directory path to strip from file paths
    pub fn new(working_dir: impl Into<String>) -> Self {
        Self { working_dir: working_dir.into() }
    }

    /// Strips the working directory prefix from a path if present.
    ///
    /// Returns the path with the working directory prefix removed, or the
    /// original path if it doesn't start with the working directory.
    fn strip_prefix(&self, path: &str) -> String {
        Path::new(path)
            .strip_prefix(&self.working_dir)
            .ok()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| path.to_string())
    }
}

impl Transformer for StripWorkingDir {
    type Value = ContextSummary;

    fn transform(&mut self, mut summary: Self::Value) -> Self::Value {
        for message in summary.messages.iter_mut() {
            for block in message.messages.iter_mut() {
                if let Some(ref mut tool_call) = block.tool_call {
                    match tool_call {
                        SummaryToolCall::FileRead { path } => {
                            *path = self.strip_prefix(path);
                        }
                        SummaryToolCall::FileUpdate { path } => {
                            *path = self.strip_prefix(path);
                        }
                        SummaryToolCall::FileRemove { path } => {
                            *path = self.strip_prefix(path);
                        }
                    }
                }
            }
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::Role;
    use crate::compact::summary::{SummaryMessage, SummaryMessageBlock as Block};

    // Helper to create a summary message with role and blocks
    fn message(role: Role, blocks: Vec<Block>) -> SummaryMessage {
        SummaryMessage { role, messages: blocks }
    }

    // Helper to create a context summary
    fn summary(messages: Vec<SummaryMessage>) -> ContextSummary {
        ContextSummary { messages }
    }

    #[test]
    fn test_empty_summary() {
        let fixture = summary(vec![]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strips_working_dir_from_file_read() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/home/user/project/src/main.rs"),
                Block::read("/home/user/project/tests/test.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![Block::read("src/main.rs"), Block::read("tests/test.rs")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strips_working_dir_from_file_update() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::update("/home/user/project/src/lib.rs"),
                Block::update("/home/user/project/README.md"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![Block::update("src/lib.rs"), Block::update("README.md")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strips_working_dir_from_file_remove() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::remove("/home/user/project/old.rs"),
                Block::remove("/home/user/project/deprecated/mod.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![Block::remove("old.rs"), Block::remove("deprecated/mod.rs")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_paths_outside_working_dir() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/home/user/project/src/main.rs"),
                Block::read("/etc/config.toml"),
                Block::update("/tmp/cache.json"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("src/main.rs"),
                Block::read("/etc/config.toml"),
                Block::update("/tmp/cache.json"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_mixed_tool_calls() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/home/user/project/src/main.rs"),
                Block::update("/home/user/project/src/lib.rs"),
                Block::remove("/home/user/project/old.rs"),
                Block::read("/other/path/file.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("src/main.rs"),
                Block::update("src/lib.rs"),
                Block::remove("old.rs"),
                Block::read("/other/path/file.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_multiple_messages_and_roles() {
        let fixture = summary(vec![
            message(
                Role::User,
                vec![Block::read("/home/user/project/src/main.rs")],
            ),
            message(
                Role::Assistant,
                vec![
                    Block::read("/home/user/project/src/lib.rs"),
                    Block::update("/home/user/project/README.md"),
                ],
            ),
            message(Role::User, vec![Block::remove("/home/user/project/old.rs")]),
        ]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![
            message(Role::User, vec![Block::read("src/main.rs")]),
            message(
                Role::Assistant,
                vec![Block::read("src/lib.rs"), Block::update("README.md")],
            ),
            message(Role::User, vec![Block::remove("old.rs")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_blocks_without_tool_calls() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::default()
                    .content("Some text content")
                    .tool_call_success(true),
                Block::read("/home/user/project/src/main.rs"),
                Block::default()
                    .content("More content")
                    .tool_call_success(false),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::default()
                    .content("Some text content")
                    .tool_call_success(true),
                Block::read("src/main.rs"),
                Block::default()
                    .content("More content")
                    .tool_call_success(false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_trailing_slash_in_working_dir() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![Block::read("/home/user/project/src/main.rs")],
        )]);
        let actual = StripWorkingDir::new("/home/user/project/").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![Block::read("src/main.rs")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_relative_paths() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("src/main.rs"),
                Block::update("./tests/test.rs"),
                Block::remove("../other/file.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("src/main.rs"),
                Block::update("./tests/test.rs"),
                Block::remove("../other/file.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }
}
