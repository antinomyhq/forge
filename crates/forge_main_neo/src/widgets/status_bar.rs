use ratatui::layout::Alignment;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};

use crate::domain::Workspace;

pub struct StatusBar {
    editor_status: Option<String>,
    agent: Option<String>,
    workspace: Workspace,
}

impl StatusBar {
    /// Create a new StatusBar with all fields
    pub fn new(agent: impl ToString, editor_status: impl ToString, workspace: Workspace) -> Self {
        Self {
            editor_status: Some(editor_status.to_string()),
            agent: Some(agent.to_string()),
            workspace,
        }
    }
}

impl<'a> From<StatusBar> for Line<'a> {
    fn from(value: StatusBar) -> Self {
        let space = Span::from(" ");
        let separator = Span::styled(" | ", Style::default().fg(Color::DarkGray));
        let mut spans = vec![space.clone()];

        // Add editor status if available
        if let Some(editor_status) = value.editor_status {
            let bg_color = if editor_status.to_lowercase() == "insert" {
                Color::Green
            } else {
                Color::White
            };
            spans.push(
                Span::from(format!(" {} ", editor_status.to_uppercase()))
                    .bg(bg_color)
                    .fg(Color::Black),
            );
            if !spans.is_empty() {
                spans.push(separator.clone());
            }
        }

        // Add agent if available
        if let Some(agent) = value.agent {
            spans.push(
                Span::from(format!(" {} ", agent.to_uppercase()))
                    .bg(Color::White)
                    .fg(Color::Black),
            );
            if !spans.is_empty() {
                spans.push(separator.clone());
            }
        }

        // Add branch information if available
        if let Some(branch) = value.workspace.current_branch {
            spans.push(Span::from(branch.to_string()).fg(Color::LightGreen));
            if !spans.is_empty() {
                spans.push(separator.clone());
            }
        }

        // Add directory information if available (show only the directory name, not
        // full path)
        if let Some(dir) = value.workspace.current_dir {
            // Add space before directory if branch was added
            let dir_name = std::path::Path::new(&dir)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&dir);
            spans.push(Span::from(format!("{dir_name} ")).fg(Color::LightCyan));
        }

        Line::from(spans).alignment(Alignment::Left).bold()
    }
}
