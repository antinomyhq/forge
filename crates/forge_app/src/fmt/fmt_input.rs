use std::convert::AsRef;
use std::path::Path;

use forge_display::TitleFormat;
use forge_domain::{Environment, Tools};

use crate::fmt::content::{ContentFormat, FormatContent};
use crate::utils::format_display_path;

impl FormatContent for Tools {
    fn to_content(&self, env: &Environment) -> Option<ContentFormat> {
        let display_path_for = |path: &str| format_display_path(Path::new(path), env.cwd.as_path());

        match self {
            Tools::ForgeToolFsRead(input) => {
                let display_path = display_path_for(&input.path);
                let is_explicit_range = input.start_line.is_some() || input.end_line.is_some();
                let mut subtitle = display_path;
                if is_explicit_range {
                    match (&input.start_line, &input.end_line) {
                        (Some(start), Some(end)) => {
                            subtitle.push_str(&format!(" [Range {start}-{end}]"));
                        }
                        (Some(start), None) => {
                            subtitle.push_str(&format!(" [Range {start}-]"));
                        }
                        (None, Some(end)) => {
                            subtitle.push_str(&format!(" [Range -{end}]"));
                        }
                        (None, None) => {}
                    }
                };
                Some(TitleFormat::debug("Read").sub_title(subtitle).into())
            }
            Tools::ForgeToolFsCreate(input) => {
                let display_path = display_path_for(&input.path);
                let title = if input.overwrite {
                    "Overwrite"
                } else {
                    "Create"
                };
                Some(TitleFormat::debug(title).sub_title(display_path).into())
            }
            Tools::ForgeToolFsSearch(input) => {
                let formatted_dir = display_path_for(&input.path);
                let title = match (&input.regex, &input.file_pattern) {
                    (Some(regex), Some(pattern)) => {
                        format!("Search for '{regex}' in '{pattern}' files at {formatted_dir}")
                    }
                    (Some(regex), None) => format!("Search for '{regex}' at {formatted_dir}"),
                    (None, Some(pattern)) => format!("Search for '{pattern}' at {formatted_dir}"),
                    (None, None) => format!("Search at {formatted_dir}"),
                };
                Some(TitleFormat::debug(title).into())
            }
            Tools::ForgeToolFsRemove(input) => {
                let display_path = display_path_for(&input.path);
                Some(TitleFormat::debug("Remove").sub_title(display_path).into())
            }
            Tools::ForgeToolFsPatch(input) => {
                let display_path = display_path_for(&input.path);
                Some(
                    TitleFormat::debug(input.operation.as_ref())
                        .sub_title(display_path)
                        .into(),
                )
            }
            Tools::ForgeToolFsUndo(input) => {
                let display_path = display_path_for(&input.path);
                Some(TitleFormat::debug("Undo").sub_title(display_path).into())
            }
            Tools::ForgeToolProcessShell(input) => Some(
                TitleFormat::debug(format!("Execute [{}]", env.shell))
                    .sub_title(&input.command)
                    .into(),
            ),
            Tools::ForgeToolNetFetch(input) => {
                Some(TitleFormat::debug("GET").sub_title(&input.url).into())
            }
            Tools::ForgeToolFollowup(input) => Some(
                TitleFormat::debug("Follow-up")
                    .sub_title(&input.question)
                    .into(),
            ),
            Tools::ForgeToolAttemptCompletion(input) => {
                Some(ContentFormat::Markdown(input.result.clone()))
            }
            Tools::ForgeToolTaskListAppend(_) => {
                Some(TitleFormat::debug("Task +1 ADD".to_string()).into())
            }
            Tools::ForgeToolTaskListAppendMultiple(input) => {
                Some(TitleFormat::debug(format!("Task +{} ADD", input.tasks.len())).into())
            }
            Tools::ForgeToolTaskListUpdate(_) => {
                Some(TitleFormat::debug("Task Update".to_string()).into())
            }
            Tools::ForgeToolTaskListList(_) => {
                Some(TitleFormat::debug("Task Read".to_string()).into())
            }
            Tools::ForgeToolTaskListClear(_) => {
                Some(TitleFormat::debug("Task Clear".to_string()).into())
            }
            Tools::ForgeToolPlanCreate(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use console::strip_ansi_codes;
    use forge_domain::{Environment, FSRead, FSWrite, Shell, Tools};
    use pretty_assertions::assert_eq;
    use url::Url;

    use super::{ContentFormat, FormatContent};

    impl ContentFormat {
        pub fn render(&self, with_timestamp: bool) -> String {
            match self {
                ContentFormat::Title(title) => title.render(with_timestamp),
                ContentFormat::PlainText(summary) => summary.clone(),
                ContentFormat::Markdown(summary) => summary.clone(),
            }
        }
    }

    fn fixture_environment() -> Environment {
        let max_bytes: f64 = 250.0 * 1024.0; // 250kb
        Environment {
            os: "linux".to_string(),
            pid: 12345,
            cwd: PathBuf::from("/home/user/project"),
            home: Some(PathBuf::from("/home/user")),
            shell: "/bin/bash".to_string(),
            base_path: PathBuf::from("/home/user/project"),
            retry_config: forge_domain::RetryConfig {
                initial_backoff_ms: 1000,
                min_delay_ms: 500,
                backoff_factor: 2,
                max_retry_attempts: 3,
                retry_status_codes: vec![429, 500, 502, 503, 504],
                max_delay: None,
                suppress_retry_errors: false,
            },
            max_search_lines: 25,
            max_search_result_bytes: max_bytes.ceil() as usize,
            fetch_truncation_limit: 55,
            max_read_size: 10,
            stdout_max_prefix_length: 10,
            stdout_max_suffix_length: 10,
            tool_timeout: 300,
            stdout_max_line_length: 2000,
            http: Default::default(),
            max_file_size: 0,
            forge_api_url: Url::parse("http://forgecode.dev/api").unwrap(),
        }
    }

    #[test]
    fn test_fs_read_basic() {
        let fixture = Tools::ForgeToolFsRead(FSRead {
            path: "/home/user/project/src/main.rs".to_string(),
            start_line: None,
            end_line: None,
            explanation: None,
        });
        let env = fixture_environment();

        let actual_content = fixture.to_content(&env);
        let rendered = actual_content.unwrap().render(false);
        let actual = strip_ansi_codes(&rendered);
        let expected = "⏺ Read src/main.rs";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_read_with_range() {
        let fixture = Tools::ForgeToolFsRead(FSRead {
            path: "/home/user/project/src/main.rs".to_string(),
            start_line: Some(10),
            end_line: Some(20),
            explanation: None,
        });
        let env = fixture_environment();

        let actual_content = fixture.to_content(&env);
        let rendered = actual_content.unwrap().render(false);
        let actual = strip_ansi_codes(&rendered);
        let expected = "⏺ Read src/main.rs [Range 10-20]";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_new_file() {
        let fixture = Tools::ForgeToolFsCreate(FSWrite {
            path: "/home/user/project/new_file.txt".to_string(),
            content: "Hello world".to_string(),
            overwrite: false,
            explanation: None,
        });
        let env = fixture_environment();

        let actual_content = fixture.to_content(&env);
        let rendered = actual_content.unwrap().render(false);
        let actual = strip_ansi_codes(&rendered);
        let expected = "⏺ Create new_file.txt";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_overwrite() {
        let fixture = Tools::ForgeToolFsCreate(FSWrite {
            path: "/home/user/project/existing_file.txt".to_string(),
            content: "Updated content".to_string(),
            overwrite: true,
            explanation: None,
        });
        let env = fixture_environment();

        let actual_content = fixture.to_content(&env);
        let rendered = actual_content.unwrap().render(false);
        let actual = strip_ansi_codes(&rendered);
        let expected = "⏺ Overwrite existing_file.txt";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_command() {
        let fixture = Tools::ForgeToolProcessShell(Shell {
            command: "ls -la".to_string(),
            cwd: PathBuf::from("/home/user/project"),
            keep_ansi: false,
            explanation: None,
        });
        let env = fixture_environment();

        let actual_content = fixture.to_content(&env);
        let rendered = actual_content.unwrap().render(false);
        let actual = strip_ansi_codes(&rendered);
        let expected = "⏺ Execute [/bin/bash] ls -la";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_with_and_without_timestamp() {
        let fixture = Tools::ForgeToolFsRead(FSRead {
            path: "/home/user/project/src/main.rs".to_string(),
            start_line: None,
            end_line: None,
            explanation: None,
        });
        let env = fixture_environment();
        let content = fixture.to_content(&env).unwrap();

        // Test render(false) - should not include timestamp
        let rendered_without = content.render(false);
        let actual_without = strip_ansi_codes(&rendered_without);
        assert!(!actual_without.contains("["));
        assert!(!actual_without.contains(":"));

        // Test render(true) - should include timestamp
        let rendered_with = content.render(true);
        let actual_with = strip_ansi_codes(&rendered_with);
        assert!(actual_with.contains("["));
        assert!(actual_with.contains(":"));
    }
}
