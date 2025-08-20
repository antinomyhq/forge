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
            .spinner
            .as_ref()
            .and_then(|spinner| spinner.timer.as_ref())
            .map(|timer| {
                Duration::milliseconds(
                    timer.current_time.timestamp_millis() - timer.start_time.timestamp_millis(),
                )
                .num_seconds()
            })
            .unwrap_or_default();

        if let Some(ref spinner) = state.spinner {
            // Set full with state
            let mut th_line = throbber_widgets_tui::Throbber::default()
                .throbber_style(ratatui::style::Style::default().fg(ratatui::style::Color::Green))
                .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                .to_line(&spinner.throbber);
            let message = spinner.message.as_deref().unwrap_or("Forging");
            let lb_line = Line::from(vec![
                Span::styled(
                    format!("{message} "),
                    Style::default().fg(Color::Green).bold(),
                ),
                Span::styled(format!("{duration}s"), Style::default()),
                Span::styled(" · Ctrl+C to interrupt", Style::default().dim()),
            ]);

            th_line.extend(lb_line);
            th_line
        } else {
            Line::from("")
        }
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
