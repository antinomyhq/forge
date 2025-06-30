use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::widgets::{Block, Borders, Widget};

/// A reusable bordered panel that wraps content with consistent styling
/// This provides a simple border around any widget content for visual
/// consistency
#[derive(Default)]
pub struct BorderedPanel<W> {
    content: W,
}

impl<W> BorderedPanel<W> {
    /// Create a new bordered panel with the given content widget
    pub fn new(content: W) -> Self {
        Self { content }
    }
}

impl<W> Widget for BorderedPanel<W>
where
    W: Widget,
{
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        // Create a bordered block
        let block = Block::bordered()
            .borders(Borders::ALL)
            .border_style(Style::default().dark_gray());

        // Render the content inside the bordered area
        self.content.render(block.inner(area), buf);

        // Render the border
        block.render(area, buf);
    }
}
