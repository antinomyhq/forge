use ratatui::layout::Alignment;
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};

pub struct StatusBar {
    mode: String,
}

impl StatusBar {
    pub fn new(mode: String) -> Self {
        Self { mode }
    }
}

impl<'a> From<StatusBar> for Line<'a> {
    fn from(value: StatusBar) -> Self {
        let space = Span::from(" ");
        Line::from(vec![
            space.clone(),
            Span::from(format!(" {} ", value.mode.to_uppercase())).bg(Color::White),
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
