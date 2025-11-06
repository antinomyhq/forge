use std::path::Path;

use crate::Transformer;
use crate::compact::summary::{ContextSummary, SummaryMessageBlock, SummaryToolCall};

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
            for block in message.blocks.iter_mut() {
                if let SummaryMessageBlock::ToolCall(tool_data) = block {
                    match &mut tool_data.call {
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
        SummaryMessage { role, blocks }
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
                Block::read(None, "/home/user/project/src/main.rs"),
                Block::read(None, "/home/user/project/tests/test.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "src/main.rs"),
                Block::read(None, "tests/test.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strips_working_dir_from_file_update() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::update(None, "/home/user/project/src/lib.rs"),
                Block::update(None, "/home/user/project/README.md"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::update(None, "src/lib.rs"),
                Block::update(None, "README.md"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strips_working_dir_from_file_remove() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::remove(None, "/home/user/project/old.rs"),
                Block::remove(None, "/home/user/project/deprecated/mod.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::remove(None, "old.rs"),
                Block::remove(None, "deprecated/mod.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_paths_outside_working_dir() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "/home/user/project/src/main.rs"),
                Block::read(None, "/etc/config.toml"),
                Block::update(None, "/tmp/cache.json"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "src/main.rs"),
                Block::read(None, "/etc/config.toml"),
                Block::update(None, "/tmp/cache.json"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_mixed_tool_calls() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "/home/user/project/src/main.rs"),
                Block::update(None, "/home/user/project/src/lib.rs"),
                Block::remove(None, "/home/user/project/old.rs"),
                Block::read(None, "/other/path/file.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "src/main.rs"),
                Block::update(None, "src/lib.rs"),
                Block::remove(None, "old.rs"),
                Block::read(None, "/other/path/file.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_multiple_messages_and_roles() {
        let fixture = summary(vec![
            message(
                Role::User,
                vec![Block::read(None, "/home/user/project/src/main.rs")],
            ),
            message(
                Role::Assistant,
                vec![
                    Block::read(None, "/home/user/project/src/lib.rs"),
                    Block::update(None, "/home/user/project/README.md"),
                ],
            ),
            message(
                Role::User,
                vec![Block::remove(None, "/home/user/project/old.rs")],
            ),
        ]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![
            message(Role::User, vec![Block::read(None, "src/main.rs")]),
            message(
                Role::Assistant,
                vec![
                    Block::read(None, "src/lib.rs"),
                    Block::update(None, "README.md"),
                ],
            ),
            message(Role::User, vec![Block::remove(None, "old.rs")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_blocks_without_tool_calls() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::content("Some text content"),
                Block::read(None, "/home/user/project/src/main.rs"),
                Block::content("More content"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::content("Some text content"),
                Block::read(None, "src/main.rs"),
                Block::content("More content"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_trailing_slash_in_working_dir() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![Block::read(None, "/home/user/project/src/main.rs")],
        )]);
        let actual = StripWorkingDir::new("/home/user/project/").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![Block::read(None, "src/main.rs")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_relative_paths() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "src/main.rs"),
                Block::update(None, "./tests/test.rs"),
                Block::remove(None, "../other/file.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("/home/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "src/main.rs"),
                Block::update(None, "./tests/test.rs"),
                Block::remove(None, "../other/file.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strips_windows_paths() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"C:\Users\user\project\src\main.rs"),
                Block::update(None, r"C:\Users\user\project\tests\test.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new(r"C:\Users\user\project").transform(fixture);

        // On Windows, paths are recognized and stripped
        #[cfg(windows)]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"src\main.rs"),
                Block::update(None, r"tests\test.rs"),
            ],
        )]);

        // On Unix, Windows paths are not recognized, so they remain unchanged
        #[cfg(not(windows))]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"C:\Users\user\project\src\main.rs"),
                Block::update(None, r"C:\Users\user\project\tests\test.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strips_windows_paths_with_forward_slashes() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "C:/Users/user/project/src/main.rs"),
                Block::update(None, "C:/Users/user/project/tests/test.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new("C:/Users/user/project").transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "src/main.rs"),
                Block::update(None, "tests/test.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strips_windows_paths_mixed_slashes() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"C:\Users\user\project\src\main.rs"),
                Block::update(None, "C:/Users/user/project/tests/test.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new(r"C:\Users\user\project").transform(fixture);

        #[cfg(windows)]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"src\main.rs"),
                Block::update(None, "tests/test.rs"),
            ],
        )]);

        #[cfg(not(windows))]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"C:\Users\user\project\src\main.rs"),
                Block::update(None, "C:/Users/user/project/tests/test.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_windows_paths_outside_working_dir() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"C:\Users\user\project\src\main.rs"),
                Block::read(None, r"D:\other\config.toml"),
                Block::update(None, r"C:\Windows\System32\file.dll"),
            ],
        )]);
        let actual = StripWorkingDir::new(r"C:\Users\user\project").transform(fixture);

        #[cfg(windows)]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"src\main.rs"),
                Block::read(None, r"D:\other\config.toml"),
                Block::update(None, r"C:\Windows\System32\file.dll"),
            ],
        )]);

        #[cfg(not(windows))]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"C:\Users\user\project\src\main.rs"),
                Block::read(None, r"D:\other\config.toml"),
                Block::update(None, r"C:\Windows\System32\file.dll"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_windows_unc_paths() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"\\server\share\project\src\main.rs"),
                Block::update(None, r"\\server\share\project\tests\test.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new(r"\\server\share\project").transform(fixture);

        #[cfg(windows)]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"src\main.rs"),
                Block::update(None, r"tests\test.rs"),
            ],
        )]);

        #[cfg(not(windows))]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"\\server\share\project\src\main.rs"),
                Block::update(None, r"\\server\share\project\tests\test.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_windows_trailing_backslash() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![Block::read(None, r"C:\Users\user\project\src\main.rs")],
        )]);
        let actual = StripWorkingDir::new(r"C:\Users\user\project\").transform(fixture);

        #[cfg(windows)]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![Block::read(None, r"src\main.rs")],
        )]);

        #[cfg(not(windows))]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![Block::read(None, r"C:\Users\user\project\src\main.rs")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_windows_case_sensitivity() {
        // On Windows, paths are case-insensitive, but we preserve the original case
        // when stripping. This test verifies case-sensitive matching behavior.
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"C:\Users\User\Project\src\main.rs"),
                Block::update(None, r"c:\users\user\project\tests\test.rs"),
            ],
        )]);
        let actual = StripWorkingDir::new(r"C:\Users\User\Project").transform(fixture);

        // On Windows: case-insensitive matching, first path strips, second doesn't
        // On Unix: case-sensitive matching, neither path strips (Windows paths not
        // recognized)
        #[cfg(windows)]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"src\main.rs"),
                Block::update(None, r"c:\users\user\project\tests\test.rs"),
            ],
        )]);

        #[cfg(not(windows))]
        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, r"C:\Users\User\Project\src\main.rs"),
                Block::update(None, r"c:\users\user\project\tests\test.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_windows_multiple_messages_and_roles() {
        let fixture = summary(vec![
            message(
                Role::User,
                vec![Block::read(None, r"C:\project\src\main.rs")],
            ),
            message(
                Role::Assistant,
                vec![
                    Block::read(None, r"C:\project\src\lib.rs"),
                    Block::update(None, r"C:\project\README.md"),
                ],
            ),
            message(Role::User, vec![Block::remove(None, r"C:\project\old.rs")]),
        ]);
        let actual = StripWorkingDir::new(r"C:\project").transform(fixture);

        #[cfg(windows)]
        let expected = summary(vec![
            message(Role::User, vec![Block::read(None, r"src\main.rs")]),
            message(
                Role::Assistant,
                vec![
                    Block::read(None, r"src\lib.rs"),
                    Block::update(None, "README.md"),
                ],
            ),
            message(Role::User, vec![Block::remove(None, "old.rs")]),
        ]);

        #[cfg(not(windows))]
        let expected = summary(vec![
            message(
                Role::User,
                vec![Block::read(None, r"C:\project\src\main.rs")],
            ),
            message(
                Role::Assistant,
                vec![
                    Block::read(None, r"C:\project\src\lib.rs"),
                    Block::update(None, r"C:\project\README.md"),
                ],
            ),
            message(Role::User, vec![Block::remove(None, r"C:\project\old.rs")]),
        ]);

        assert_eq!(actual, expected);
    }
}
