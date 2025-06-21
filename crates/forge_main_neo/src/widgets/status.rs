use ratatui::layout::Alignment;
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};

pub struct StatusBar {
    editor_status: String,
    agent: String,
}

impl StatusBar {
    pub fn new(agent: impl ToString, editor_status: impl ToString) -> Self {
        Self {
            editor_status: editor_status.to_string(),
            agent: agent.to_string(),
        }
    }
}

impl<'a> From<StatusBar> for Line<'a> {
    fn from(value: StatusBar) -> Self {
        let space = Span::from(" ");
        Line::from(vec![
            space.clone(),
            Span::from(format!(" {} ", value.agent.to_uppercase())).bg(Color::LightYellow),
            space.clone(),
            Span::from(format!(" {} ", value.editor_status.to_uppercase())).bg(Color::White),
            space.clone(),
            Span::from("code-forge").fg(Color::LightCyan),
            space.clone(),
            Span::from("ratatui-integration-apr-12").fg(Color::LightYellow),
            space.clone(),
        ])
        .alignment(Alignment::Left)
        .bold()
    }
}
