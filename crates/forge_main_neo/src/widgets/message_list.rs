use ansi_to_tui::IntoText;
use color_eyre::owo_colors::OwoColorize;
use forge_api::ChatResponse;
use ratatui::layout::Size;
use ratatui::prelude::Widget;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, StatefulWidget, Wrap};
use tui_scrollview::ScrollView;

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
                ChatResponse::TaskMessage { text, is_md } => {
                    if *is_md {
                        let rendered_text = forge_display::MarkdownFormat::new().render(text);
                        match rendered_text.into_text() {
                            Ok(text) => text.lines.into_iter(),
                            Err(_) => vec![Line::raw(rendered_text)].into_iter(),
                        }
                    } else {
                        match text.clone().into_text() {
                            Ok(text) => text.lines.into_iter(),
                            Err(_) => vec![Line::raw(text.clone())].into_iter(),
                        }
                    }
                }
                ChatResponse::ToolCallStart(_) => vec![].into_iter(),
                ChatResponse::ToolCallEnd(_) => vec![].into_iter(),
                ChatResponse::Usage(_) => vec![].into_iter(),
                ChatResponse::Interrupt { reason: _ } => {
                    todo!()
                }
                ChatResponse::TaskReasoning { content } => {
                    if !content.trim().is_empty() {
                        let dimmed_content = content.dimmed().to_string();
                        match dimmed_content.into_text() {
                            Ok(text) => text.lines.into_iter(),
                            Err(_) => vec![Line::raw(dimmed_content)].into_iter(),
                        }
                    } else {
                        vec![].into_iter()
                    }
                }

                ChatResponse::RetryAttempt { cause: _, duration: _ } => {
                    todo!()
                }
                ChatResponse::TaskComplete { .. } => vec![].into_iter(),
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

        // Calculate content height based on the number of lines
        let content_height = lines.len() as u16;

        // Calculate width - reserve space for scrollbar if content exceeds area height
        let width = if content_height > area.height {
            area.width.saturating_sub(1)
        } else {
            area.width
        };

        // Create the scroll view with proper content dimensions
        let mut scroll_view = ScrollView::new(Size::new(width, content_height))
            .horizontal_scrollbar_visibility(tui_scrollview::ScrollbarVisibility::Never);

        // Render the paragraph into the scroll view's buffer
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        paragraph.render(scroll_view.area(), scroll_view.buf_mut());

        // Render the scroll view into the main buffer
        scroll_view.render(area, buf, &mut state.message_scroll_state);
    }
}
