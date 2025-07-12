use chrono::Duration;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{StatefulWidget, Widget};

use crate::domain::State;

#[derive(Clone, Default)]
pub struct Spinner {}

impl Spinner {
    pub fn to_line(&self, state: &State) -> Line<'_> {
        let duration = state
            .timer
            .as_ref()
            .map(|timer| {
                Duration::milliseconds(
                    timer.current_time.timestamp_millis() - timer.start_time.timestamp_millis(),
                )
                .num_seconds()
            })
            .unwrap_or_default();
        // Set full with state
        let mut th_line = throbber_widgets_tui::Throbber::default()
            .throbber_style(ratatui::style::Style::default().fg(ratatui::style::Color::Green))
            .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
            .to_line(&state.spinner);
        let lb_line = Line::from(vec![
            Span::styled("Forging ", Style::default().fg(Color::Green).bold()),
            Span::styled(format!("{duration}s"), Style::default()),
            Span::styled(" · Ctrl+C to interrupt", Style::default().dim()),
        ]);

        th_line.extend(lb_line);
        th_line
    }
}

impl StatefulWidget for Spinner {
    type State = State;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut State,
    ) where
        Self: Sized,
    {
        self.to_line(state).render(area, buf);
    }
}
