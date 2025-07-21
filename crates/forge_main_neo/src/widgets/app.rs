use ratatui::widgets::StatefulWidget;

use crate::domain::State;
use crate::widgets::chat::ChatWidget;

#[derive(Clone, Default)]
pub struct App;

impl StatefulWidget for App {
    type State = State;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut State,
    ) where
        Self: Sized,
    {
        ChatWidget.render(area, buf, state);
    }
}
