use std::path::{Path, PathBuf};

use forge_domain::{ChatResponseContent, Environment, ToolCatalog};

use crate::fmt::content::FormatContent;
use crate::utils::format_display_path;

impl FormatContent for ToolCatalog {
    fn to_content(&self, env: &Environment) -> Option<ChatResponseContent> {
        let display_path_for = |path: &str| format_display_path(Path::new(path), env.cwd.as_path());

        let formatted = match self {
            ToolCatalog::Read(input) => {
                let display_path = display_path_for(&input.path);
                let is_explicit_range = input.start_line.is_some() || input.end_line.is_some();
                let mut result = format!("Read {}", display_path);
                if is_explicit_range {
                    match (&input.start_line, &input.end_line) {
                        (Some(start), Some(end)) => {
                            result.push_str(&format!(":{start}-{end}"));
                        }
                        (Some(start), None) => {
                            result.push_str(&format!(":{start}"));
                        }
                        (None, Some(end)) => {
                            result.push_str(&format!(":1-{end}"));
                        }
                        (None, None) => {}
                    }
                };
                result
            }
            ToolCatalog::ReadImage(input) => {
                let display_path = display_path_for(&input.path);
                format!("Image {}", display_path)
            }
            ToolCatalog::Write(input) => {
                let path = PathBuf::from(&input.path);
                let display_path = display_path_for(&input.path);
                let title = match (path.exists(), input.overwrite) {
                    (true, true) => "Overwrite",
                    (true, false) => {
                        // Case: file exists but overwrite is false then we throw error from tool,
                        // so it's good idea to not print anything on CLI.
                        return None;
                    }
                    (false, _) => "Create",
                };
                format!("{} {}", title, display_path)
            }
            ToolCatalog::Search(input) => {
                let formatted_dir = display_path_for(&input.path);
                match (&input.regex, &input.file_pattern) {
                    (Some(regex), Some(pattern)) => {
                        format!("Search for '{regex}' in '{pattern}' files at {formatted_dir}")
                    }
                    (Some(regex), None) => format!("Search for '{regex}' at {formatted_dir}"),
                    (None, Some(pattern)) => format!("Search for '{pattern}' at {formatted_dir}"),
                    (None, None) => format!("Search at {formatted_dir}"),
                }
            }
            ToolCatalog::SemSearch(input) => {
                let pairs: Vec<_> = input
                    .queries
                    .iter()
                    .map(|item| item.query.as_str())
                    .collect();
                format!("Codebase Search [{}]", pairs.join(" · "))
            }
            ToolCatalog::Remove(input) => {
                let display_path = display_path_for(&input.path);
                format!("Remove {}", display_path)
            }
            ToolCatalog::Patch(input) => {
                let display_path = display_path_for(&input.path);
                format!("{} {}", input.operation.as_ref(), display_path)
            }
            ToolCatalog::Undo(input) => {
                let display_path = display_path_for(&input.path);
                format!("Undo {}", display_path)
            }
            ToolCatalog::Shell(input) => {
                format!("Execute [{}] {}", env.shell, input.command)
            }
            ToolCatalog::Fetch(input) => {
                format!("GET {}", input.url)
            }
            ToolCatalog::Followup(input) => {
                format!("Follow-up: {}", input.question)
            }
            ToolCatalog::Plan(_) => return None,
            ToolCatalog::Skill(input) => {
                format!("Skill: {}", input.name.to_lowercase())
            }
        };

        Some(ChatResponseContent::ToolInput(formatted))
    }
}
