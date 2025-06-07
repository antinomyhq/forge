use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};

pub struct KBShortcutsLine;

impl<'a> From<KBShortcutsLine> for Line<'a> {
    fn from(_val: KBShortcutsLine) -> Self {
        Line::from(vec![
            Span::from(" EXIT "),
            Span::from("<CTRL+D>").bg(Color::Gray),
            Span::from(" STOP "),
            Span::from("<CTRL+C>").bg(Color::Gray),
            Span::from(" RUN "),
            Span::from("<CTRL+R>").bg(Color::Gray),
            Span::from(" MODE "),
            Span::from("<CTRL+M>").bg(Color::Gray),
            Span::from(" "),
        ])
        .centered()
        .bold()
    }
}
