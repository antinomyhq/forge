use std::path::Path;

use forge_display::TitleFormat;
use forge_domain::{Environment, Tools};

use crate::utils::display_path;

pub trait InputTitle {
    fn to_title(&self, env: &Environment) -> TitleFormat;
}

impl InputTitle for Tools {
    fn to_title(&self, env: &Environment) -> TitleFormat {
        match self {
            Tools::ForgeToolFsRead(input) => {
                let display_path = display_path(env, Path::new(&input.path));
                let is_explicit_range = input.start_line.is_some() || input.end_line.is_some();

                let title = if is_explicit_range {
                    "Read (Range)"
                } else {
                    "Read"
                };

                TitleFormat::debug(title).sub_title(display_path)
            }
            Tools::ForgeToolFsCreate(input) => {
                let display_path = display_path(env, Path::new(&input.path));
                let title = if input.overwrite {
                    "Overwrite"
                } else {
                    "Create"
                };
                TitleFormat::debug(title).sub_title(display_path)
            }
            Tools::ForgeToolFsSearch(input) => {
                let formatted_dir = display_path(env, Path::new(&input.path));
                let title = match (&input.regex, &input.file_pattern) {
                    (Some(regex), Some(pattern)) => {
                        format!("Search for '{regex}' in '{pattern}' files at {formatted_dir}")
                    }
                    (Some(regex), None) => format!("Search for '{regex}' at {formatted_dir}"),
                    (None, Some(pattern)) => format!("Search for '{pattern}' at {formatted_dir}"),
                    (None, None) => format!("Search at {formatted_dir}"),
                };
                TitleFormat::debug(title)
            }
            Tools::ForgeToolFsRemove(input) => {
                let display_path = display_path(env, Path::new(&input.path));
                TitleFormat::debug("Remove").sub_title(display_path)
            }
            Tools::ForgeToolFsPatch(input) => {
                let display_path = display_path(env, Path::new(&input.path));
                TitleFormat::debug("Patch").sub_title(display_path)
            }
            Tools::ForgeToolFsUndo(input) => {
                let display_path = display_path(env, Path::new(&input.path));
                TitleFormat::debug("Undo").sub_title(display_path)
            }
            Tools::ForgeToolProcessShell(input) => {
                TitleFormat::debug(format!("Execute [{}]", env.shell)).sub_title(&input.command)
            }
            Tools::ForgeToolNetFetch(input) => TitleFormat::debug("GET").sub_title(&input.url),
            Tools::ForgeToolFollowup(input) => {
                TitleFormat::debug("Follow-up").sub_title(&input.question)
            }
            Tools::ForgeToolAttemptCompletion(input) => {
                TitleFormat::debug("Completion").sub_title(&input.result)
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use console::strip_ansi_codes;
    use forge_domain::{Environment, FSRead, FSWrite, Shell, Tools};
    use pretty_assertions::assert_eq;

    use super::InputTitle;

    fn fixture_environment() -> Environment {
        Environment {
            os: "linux".to_string(),
            pid: 12345,
            cwd: PathBuf::from("/home/user/project"),
            home: Some(PathBuf::from("/home/user")),
            shell: "/bin/bash".to_string(),
            base_path: PathBuf::from("/home/user/project"),
            provider: forge_domain::Provider::OpenAI {
                url: "https://api.openai.com/v1/".parse().unwrap(),
                key: Some("test-key".to_string()),
            },
            retry_config: forge_domain::RetryConfig {
                initial_backoff_ms: 1000,
                min_delay_ms: 500,
                backoff_factor: 2,
                max_retry_attempts: 3,
                retry_status_codes: vec![429, 500, 502, 503, 504],
            },
            max_search_lines: 25,
            fetch_truncation_limit: 55,
            max_read_size: 10,
            stdout_max_prefix_length: 10,
            stdout_max_suffix_length: 10,
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

        let title_format = fixture.to_title(&env);
        let rendered = title_format.render(false);
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

        let title_format = fixture.to_title(&env);
        let rendered = title_format.render(false);
        let actual = strip_ansi_codes(&rendered);
        let expected = "⏺ Read (Range) src/main.rs";

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

        let title_format = fixture.to_title(&env);
        let rendered = title_format.render(false);
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

        let title_format = fixture.to_title(&env);
        let rendered = title_format.render(false);
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

        let title_format = fixture.to_title(&env);
        let rendered = title_format.render(false);
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
        let title_format = fixture.to_title(&env);

        // Test render(false) - should not include timestamp
        let rendered_without_timestamp = title_format.render(false);
        let actual_without = strip_ansi_codes(&rendered_without_timestamp);
        assert!(!actual_without.contains("["));
        assert!(!actual_without.contains(":"));

        // Test render(true) - should include timestamp
        let rendered_with_timestamp = title_format.render(true);
        let actual_with = strip_ansi_codes(&rendered_with_timestamp);
        assert!(actual_with.contains("["));
        assert!(actual_with.contains(":"));

        // Test Display trait - should include timestamp (same as render(true))
        let display_string = title_format.to_string();
        let actual_display = strip_ansi_codes(&display_string);
        assert!(actual_display.contains("["));
        assert!(actual_display.contains(":"));
    }
}
