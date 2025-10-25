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
                let title = if input.overwrite {
                    "Overwrite"
                } else {
                    "Create"
                };
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
