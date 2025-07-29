use forge_api::ModelId;
use ratatui::layout::Alignment;
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};

use crate::domain::Workspace;

pub struct StatusBar {
    editor_status: Option<String>,
    agent: Option<String>,
    workspace: Workspace,
    current_model: Option<ModelId>,
}

impl StatusBar {
    /// Create a new StatusBar with all fields
    pub fn new(
        agent: impl ToString,
        editor_status: impl ToString,
        workspace: Workspace,
        current_model: Option<ModelId>,
    ) -> Self {
        Self {
            editor_status: Some(editor_status.to_string()),
            agent: Some(agent.to_string()),
            workspace,
            current_model,
        }
    }
}

impl<'a> From<StatusBar> for Line<'a> {
    fn from(value: StatusBar) -> Self {
        let space = Span::from(" ");
        let mut spans = vec![space.clone()];

        // Add editor status if available
        if let Some(editor_status) = value.editor_status {
            let bg_color = if editor_status.to_lowercase() == "insert" {
                Color::Green
            } else {
                Color::White
            };
            spans.push(Span::from(format!(" {} ", editor_status.to_uppercase())).bg(bg_color));
            spans.push(space.clone());
        }

        // Add agent if available
        if let Some(agent) = value.agent {
            spans.push(Span::from(format!(" {} ", agent.to_uppercase())).bg(Color::White));
            spans.push(space.clone());
        }

        // Add current model if available
        if let Some(model) = value.current_model {
            spans.push(Span::from(format!(" Model: {} ", model.as_str())).bg(Color::Cyan));
            spans.push(space.clone());
        }

        // Check if we have both branch and directory for spacing logic
        let has_branch = value.workspace.current_branch.is_some();

        // Add branch information if available
        if let Some(branch) = value.workspace.current_branch {
            spans.push(Span::from(branch.to_string()).fg(Color::LightGreen));
        }

        // Add directory information if available (show only the directory name, not
        // full path)
        if let Some(dir) = value.workspace.current_dir {
            // Add space before directory if branch was added
            if has_branch {
                spans.push(space.clone());
            }
            let dir_name = std::path::Path::new(&dir)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&dir);
            spans.push(Span::from(format!("{dir_name} ")).fg(Color::LightCyan));
        }

        Line::from(spans).alignment(Alignment::Left).bold()
    }
}
