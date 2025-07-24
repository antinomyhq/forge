use edtui::{EditorTheme, EditorView};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Padding, StatefulWidget, Widget};

use crate::domain::{EditorStateExt, State};
use crate::widgets::history::HistoryWidget;
use crate::widgets::message_list::MessageList;
use crate::widgets::spotlight::SpotlightWidget;
use crate::widgets::status_bar::StatusBar;
use crate::widgets::welcome::WelcomeWidget;

/// Chat widget that handles the chat interface with editor and message list
#[derive(Clone, Default)]
pub struct ChatWidget;

impl StatefulWidget for ChatWidget {
    type State = State;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut State,
    ) where
        Self: Sized,
    {
        // Create chat layout with messages area at top and user input area at bottom
        let chat_layout = Layout::new(
            Direction::Vertical,
            [Constraint::Fill(0), Constraint::Max(5)],
        );
        let [messages_area, user_area] = chat_layout.areas(area);

        // Messages area block (now at top)
        let message_block = Block::new();

        // Render welcome widget if no messages, otherwise render message list
        if state.messages.is_empty() {
            WelcomeWidget.render(message_block.inner(messages_area), buf, state);
        } else {
            MessageList.render(message_block.inner(messages_area), buf, state);
        }

        if state.spotlight.is_visible {
            SpotlightWidget.render(messages_area, buf, state)
        }

        // User input area block with status bar (now at bottom)
        let user_block = Block::bordered()
            .padding(Padding::new(0, 0, 0, 1))
            .border_style(Style::default().dark_gray())
            .title_bottom(if state.history_search.is_active {
                Line::from(state.history_search.status_text())
            } else {
                StatusBar::new("FORGE", state.editor.mode.name(), state.workspace.clone()).into()
            });

        // When history search is active, show the current match in the editor
        if state.history_search.is_active {
            if let Some(current_match) = state.history_search.current_match() {
                // Temporarily set the editor text to show the current match
                let original_text = state.editor.get_text();
                state.editor.set_text_insert_mode(current_match);

                EditorView::new(&mut state.editor)
                    .theme(
                        EditorTheme::default()
                            .base(Style::reset())
                            .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                            .hide_status_line(),
                    )
                    .wrap(true)
                    .render(user_block.inner(user_area), buf);

                // Restore original text
                state.editor.set_text_insert_mode(original_text);
            } else {
                EditorView::new(&mut state.editor)
                    .theme(
                        EditorTheme::default()
                            .base(Style::reset())
                            .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                            .hide_status_line(),
                    )
                    .wrap(true)
                    .render(user_block.inner(user_area), buf);
            }
        } else {
            EditorView::new(&mut state.editor)
                .theme(
                    EditorTheme::default()
                        .base(Style::reset())
                        .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                        .hide_status_line(),
                )
                .wrap(true)
                .render(user_block.inner(user_area), buf);
            // Render autocomplete suggestion overlay using HistoryWidget
            HistoryWidget.render(user_block.inner(user_area), buf, state);
        }

        // Render blocks
        message_block.render(messages_area, buf);
        user_block.render(user_area, buf);
    }
}
