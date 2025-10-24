use std::path::Path;

use forge_domain::{ChatResponseContent, Environment, TitleFormat, Tools};

use crate::fmt::content::FormatContent;
use crate::utils::format_display_path;

impl FormatContent for Tools {
    fn to_content(&self, env: &Environment) -> Option<ChatResponseContent> {
        let display_path_for = |path: &str| format_display_path(Path::new(path), env.cwd.as_path());

        match self {
            Tools::Read(input) => {
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
            Tools::ReadImage(input) => {
                let display_path = display_path_for(&input.path);
                Some(TitleFormat::debug("Image").sub_title(display_path).into())
            }
            Tools::Write(input) => {
                let display_path = display_path_for(&input.path);
                let file_exists = Path::new(&input.path).exists();
                let title = if file_exists { "Overwrite" } else { "Create" };
                Some(TitleFormat::debug(title).sub_title(display_path).into())
            }
            Tools::Search(input) => {
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
            Tools::Remove(input) => {
                let display_path = display_path_for(&input.path);
                Some(TitleFormat::debug("Remove").sub_title(display_path).into())
            }
            Tools::Patch(input) => {
                let display_path = display_path_for(&input.path);
                Some(
                    TitleFormat::debug(input.operation.as_ref())
                        .sub_title(display_path)
                        .into(),
                )
            }
            Tools::Undo(input) => {
                let display_path = display_path_for(&input.path);
                Some(TitleFormat::debug("Undo").sub_title(display_path).into())
            }
            Tools::Shell(input) => Some(
                TitleFormat::debug(format!("Execute [{}]", env.shell))
                    .sub_title(&input.command)
                    .into(),
            ),
            Tools::Fetch(input) => Some(TitleFormat::debug("GET").sub_title(&input.url).into()),
            Tools::Followup(input) => Some(
                TitleFormat::debug("Follow-up")
                    .sub_title(&input.question)
                    .into(),
            ),
            Tools::Plan(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use forge_domain::FSWrite;
    use tempfile::TempDir;

    use super::*;

    fn setup_env() -> (Environment, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let env = Environment {
            os: "test".to_string(),
            pid: 1,
            cwd: temp_dir.path().to_path_buf(),
            home: None,
            shell: "bash".to_string(),
            base_path: temp_dir.path().to_path_buf(),
            forge_api_url: "http://localhost".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 100,
            max_search_result_bytes: 1000,
            fetch_truncation_limit: 1000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            stdout_max_line_length: 100,
            max_read_size: 1000,
            http: Default::default(),
            max_file_size: 1000,
            max_image_size: 1000,
            tool_timeout: 300,
            auto_open_dump: false,
            custom_history_path: None,
            max_conversations: 100,
        };
        (env, temp_dir)
    }

    #[test]
    fn test_write_shows_create_for_new_file() {
        let (env, temp_dir) = setup_env();
        let new_file_path = temp_dir.path().join("new_file.txt");

        let input = FSWrite {
            path: new_file_path.to_string_lossy().to_string(),
            content: "test content".to_string(),
            overwrite: false,
        };

        let tool = Tools::Write(input);
        let actual = tool.to_content(&env);

        assert!(actual.is_some());
        let content = actual.unwrap();
        if let ChatResponseContent::Title(title) = content {
            assert_eq!(title.title, "Create");
        } else {
            panic!("Expected Title content");
        }
    }

    #[test]
    fn test_write_shows_overwrite_for_existing_file() {
        let (env, temp_dir) = setup_env();
        let existing_file_path = temp_dir.path().join("existing_file.txt");

        // Create the file first
        fs::write(&existing_file_path, "existing content").unwrap();

        let input = FSWrite {
            path: existing_file_path.to_string_lossy().to_string(),
            content: "new content".to_string(),
            overwrite: true,
        };

        let tool = Tools::Write(input);
        let actual = tool.to_content(&env);

        assert!(actual.is_some());
        let content = actual.unwrap();
        if let ChatResponseContent::Title(title) = content {
            assert_eq!(title.title, "Overwrite");
        } else {
            panic!("Expected Title content");
        }
    }

    #[test]
    fn test_write_shows_create_for_new_file_with_overwrite_flag() {
        let (env, temp_dir) = setup_env();
        let new_file_path = temp_dir.path().join("new_file.txt");

        // File doesn't exist, but overwrite flag is set to true
        let input = FSWrite {
            path: new_file_path.to_string_lossy().to_string(),
            content: "test content".to_string(),
            overwrite: true,
        };

        let tool = Tools::Write(input);
        let actual = tool.to_content(&env);

        assert!(actual.is_some());
        let content = actual.unwrap();
        if let ChatResponseContent::Title(title) = content {
            // Should show "Create" because file doesn't exist, regardless of overwrite flag
            assert_eq!(title.title, "Create");
        } else {
            panic!("Expected Title content");
        }
    }
}
