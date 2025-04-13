use ratatui::crossterm::event::KeyEvent;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;
use tui_textarea::TextArea;

#[derive(Debug)]
pub struct ForgeInput<'a> {
    input: TextArea<'a>,
}

impl<'a> Default for ForgeInput<'a> {
    fn default() -> Self {
        Self { input: ForgeInput::create_text_area() }
    }
}

impl Widget for &ForgeInput<'_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        self.input.render(area, buf);
    }
}

impl<'a> ForgeInput<'a> {
    fn create_text_area() -> TextArea<'a> {
        let mut input = TextArea::default();
        input.set_placeholder_text("Enter prompt here...");
        input.set_placeholder_style(Style::default().fg(Color::DarkGray));
        input
    }

    pub fn input(&mut self, key: KeyEvent) {
        self.input.input(key);
    }

    pub fn text(&self) -> Vec<String> {
        self.input.lines().to_vec()
    }

    pub fn clear(&mut self) {
        self.input = ForgeInput::create_text_area();
    }
}
