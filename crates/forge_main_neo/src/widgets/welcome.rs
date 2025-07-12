use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, StatefulWidget, Widget, Wrap};

use crate::domain::State;

/// Welcome widget that displays the banner and keyboard shortcuts when no
/// messages are present
#[derive(Default)]
pub struct WelcomeWidget;

impl StatefulWidget for WelcomeWidget {
    type State = State;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        _state: &mut State,
    ) where
        Self: Sized,
    {
        let [top_layout, bottom_layout] =
            Layout::vertical([Constraint::Max(8), Constraint::Fill(1)]).areas(area);
        let [left_layout, right_layout] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(bottom_layout);

        // Create banner and welcome content for top section
        let mut banner_lines = Vec::new();

        // Add banner lines
        for line in include_str!("./banner.txt").lines() {
            banner_lines.push(Line::raw(line));
        }

        // Render banner and welcome message in top section
        Paragraph::new(banner_lines)
            .style(Style::new().fg(Color::Yellow))
            .centered()
            .wrap(Wrap { trim: false })
            .render(top_layout, buf);

        // Create keyboard shortcuts for bottom section
        let shortcuts = vec![
            ("CTRL+D", "Exit application"),
            ("TAB", "Navigate to next view"),
            ("SHIFT+TAB", "Navigate to previous view"),
            ("ENTER", "Submit message (in Chat mode)"),
            ("ESC", "Switch between modes"),
        ];

        // Create left column with right-aligned shortcuts
        let mut left_lines = Vec::new();
        for (shortcut, _) in &shortcuts {
            left_lines.push(
                Line::from(vec![Span::styled(
                    format!("<{shortcut}> "),
                    Style::default().cyan(),
                )])
                .alignment(Alignment::Right),
            );
        }

        // Create right column with left-aligned descriptions
        let mut right_lines = Vec::new();
        for (_, description) in &shortcuts {
            right_lines.push(Line::from(vec![Span::styled(
                *description,
                Style::default().dim(),
            )]));
        }

        // Render shortcuts in two columns
        Paragraph::new(left_lines)
            .wrap(Wrap { trim: false })
            .render(left_layout, buf);

        Paragraph::new(right_lines)
            .wrap(Wrap { trim: false })
            .render(right_layout, buf);
    }
}
