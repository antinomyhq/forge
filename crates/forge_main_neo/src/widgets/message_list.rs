use forge_api::ChatResponse;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, StatefulWidget, Widget, Wrap};

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
                ChatResponse::Text { text, is_complete, is_md, is_summary: _ } => {
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
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}
