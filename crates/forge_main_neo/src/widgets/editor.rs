use edtui::{EditorTheme, EditorView};
use ratatui::style::{Color, Style};
use ratatui::widgets::{StatefulWidget, Widget};
use crate::domain::State;

#[derive(Default)]
pub struct EditorWidget;

impl StatefulWidget for EditorWidget {
    type State = State;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        EditorView::new(&mut state.editor)
            .theme(
                EditorTheme::default()
                    .base(Style::reset())
                    .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                    .hide_status_line(),
            )
            .wrap(true)
            .render(area, buf);
    }
}