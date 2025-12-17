use forge_display::DiffFormat;
use forge_domain::{ChatResponseContent, Environment};

use crate::fmt::content::FormatContent;
use crate::operation::ToolOperation;
use crate::utils::format_display_path;

impl FormatContent for ToolOperation {
    fn to_content(&self, env: &Environment) -> Option<ChatResponseContent> {
        match self {
            ToolOperation::FsRead { .. } => None,
            ToolOperation::ImageRead { .. } => None,
            ToolOperation::FsCreate { input, output } => {
                if let Some(ref before) = output.before {
                    let after = &input.content;
                    Some(ChatResponseContent::ToolOutput(
                        DiffFormat::format(before, after).diff().to_string(),
                    ))
                } else {
                    None
                }
            }
            ToolOperation::FsRemove { .. } => None,
            ToolOperation::FsSearch { .. } => None,
            ToolOperation::CodebaseSearch { .. } => None,
            ToolOperation::FsPatch { output, .. } => Some(ChatResponseContent::ToolOutput(
                DiffFormat::format(&output.before, &output.after)
                    .diff()
                    .to_string(),
            )),
            ToolOperation::FsUndo { .. } => None,
            ToolOperation::NetFetch { .. } => None,
            ToolOperation::Shell { .. } => None,
            ToolOperation::FollowUp { .. } => None,
            ToolOperation::PlanCreate { output, .. } => {
                Some(ChatResponseContent::ToolOutput(format!(
                    "Plan created at: {}",
                    format_display_path(&output.path, &env.cwd)
                )))
            }
            ToolOperation::Skill { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use console::strip_ansi_codes;
    use forge_display::DiffFormat;
    use forge_domain::{ChatResponseContent, Environment, PatchOperation};
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;

    use super::FormatContent;
    use crate::operation::ToolOperation;
    use crate::{Content, FsCreateOutput, PatchOutput, ReadOutput};

    fn fixture_environment() -> Environment {
        use fake::{Fake, Faker};
        let max_bytes: f64 = 250.0 * 1024.0; // 250 KB
        let fixture: Environment = Faker.fake();
        fixture
            .max_search_lines(25)
            .max_search_result_bytes(max_bytes.ceil() as usize)
            .fetch_truncation_limit(55)
            .max_read_size(10)
            .stdout_max_prefix_length(10)
            .stdout_max_suffix_length(10)
            .max_file_size(0)
    }

    #[test]
    fn test_fs_read_single_line() {
        let content = "Hello, world!";
        let fixture = ToolOperation::FsRead {
            input: forge_domain::FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 1,
                end_line: 1,
                total_lines: 5,
                content_hash: crate::compute_hash(content),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_fs_create_overwrite() {
        let content = "new content";
        let fixture = ToolOperation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/project/existing_file.txt".to_string(),
                content: content.to_string(),
                overwrite: true,
            },
            output: FsCreateOutput {
                path: "/home/user/project/existing_file.txt".to_string(),
                before: Some("old content".to_string()),
                warning: None,
                content_hash: crate::compute_hash(content),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = Some(ChatResponseContent::ToolOutput(
            DiffFormat::format("old content", "new content")
                .diff()
                .to_string(),
        ));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_patch_success() {
        let after_content = "Hello universe\nThis is a test\nNew line";
        let fixture = ToolOperation::FsPatch {
            input: forge_domain::FSPatch {
                path: "/home/user/project/test.txt".to_string(),
                search: Some("Hello world".to_string()),
                content: "Hello universe".to_string(),
                operation: PatchOperation::Replace,
            },
            output: PatchOutput {
                warning: None,
                before: "Hello world\nThis is a test".to_string(),
                after: after_content.to_string(),
                content_hash: crate::compute_hash(after_content),
            },
        };
        let env = fixture_environment();
        let actual = fixture.to_content(&env).unwrap();
        let actual = strip_ansi_codes(actual.as_str());
        assert_snapshot!(actual)
    }
}
