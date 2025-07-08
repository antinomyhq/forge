use color_eyre::owo_colors::OwoColorize;
use forge_api::ChatResponse;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, StatefulWidget, Widget, Wrap};

use crate::domain::{Message, State};
use crate::widgets::spinner::Spinner;

#[derive(Default)]
pub struct MessageList;

fn messages_to_lines(messages: &[Message]) -> Vec<Line<'_>> {
    messages
        .iter()
        .flat_map(|message| match message {
            Message::User(content) => vec![Line::from(vec![
                Span::styled("❯ ", Style::default().green()),
                Span::styled(content, Style::default().cyan().bold()),
            ])]
            .into_iter(),
            Message::Assistant(response) => match response {
                ChatResponse::Text { text, is_complete, is_md } => {
                    if *is_complete {
                        if *is_md {
                            // Use Text::from() which handles newlines automatically and more
                            // efficiently
                            let rendered_text = forge_display::MarkdownFormat::new().render(text);
                            Text::from(rendered_text).lines.into_iter()
                        } else {
                            vec![Line::raw(text.clone())].into_iter()
                        }
                    } else {
                        vec![].into_iter()
                    }
                }
                ChatResponse::ToolCallStart(_) => vec![].into_iter(),
                ChatResponse::ToolCallEnd(_) => vec![].into_iter(),
                ChatResponse::Usage(_) => vec![].into_iter(),
                ChatResponse::Interrupt { reason: _ } => {
                    todo!()
                }
                ChatResponse::Reasoning { content } => {
                    if !content.trim().is_empty() {
                        Text::from(content.dimmed().to_string()).lines.into_iter()
                    } else {
                        vec![].into_iter()
                    }
                }
                ChatResponse::Summary { content } => {
                    if !content.trim().is_empty() {
                        let rendered_text = forge_display::MarkdownFormat::new().render(content);
                        Text::from(rendered_text).lines.into_iter()
                    } else {
                        vec![].into_iter()
                    }
                }
                ChatResponse::RetryAttempt { cause: _, duration: _ } => {
                    todo!()
                }
            },
        })
        .collect()
}

impl StatefulWidget for MessageList {
    type State = State;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut State,
    ) where
        Self: Sized,
    {
        let mut lines = messages_to_lines(&state.messages);
        let s = Spinner::default();
        if state.show_spinner {
            lines.push(s.to_line(state));
        }

        // Calculate scroll position
        let total_lines = lines.len();
        let visible_lines = area.height as usize;

        // Manual scroll: ensure offset is within bounds
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let scroll_offset = state.chat_scroll.vertical_scroll.min(max_scroll);

        // Update the state to reflect the actual clamped scroll position
        // This prevents accumulation of invalid scroll values
        state.chat_scroll.vertical_scroll = scroll_offset;

        // Split area for content and scrollbar
        let content_area = ratatui::layout::Rect {
            x: area.x,
            y: area.y,
            width: area.width.saturating_sub(1), // Leave space for scrollbar
            height: area.height,
        };

        let scrollbar_area = ratatui::layout::Rect {
            x: area.x + area.width.saturating_sub(1), // Position at right edge
            y: area.y,
            width: 1,
            height: area.height,
        };

        // Update the persistent ScrollbarState with current content length and position
        state.chat_scroll.vertical_scroll_state = state
            .chat_scroll
            .vertical_scroll_state
            .content_length(max_scroll)
            .position(scroll_offset);

        // Create paragraph with scroll
        // In Ratatui, scroll parameter is (vertical_offset, horizontal_offset)
        // For bottom-up scrolling, we use the scroll_offset directly
        // Ensure scroll_offset doesn't exceed u16::MAX
        let safe_scroll_offset = scroll_offset as u16;

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((safe_scroll_offset, 0))
            .render(content_area, buf);

        // Render scrollbar if there's scrollable content
        if total_lines > visible_lines {
            Scrollbar::new(ScrollbarOrientation::VerticalRight).render(
                scrollbar_area,
                buf,
                &mut state.chat_scroll.vertical_scroll_state,
            );
        }
    }
}
