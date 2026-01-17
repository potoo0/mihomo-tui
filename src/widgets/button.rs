use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

#[derive(Debug, Clone)]
pub struct Button<'a> {
    label: &'a str,
    active: bool,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label, active: false }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

impl Widget for Button<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style =
            if self.active { Style::default().fg(Color::LightBlue) } else { Style::default() };
        let block = Block::bordered().border_type(BorderType::Rounded).border_style(style);

        let inner = block.inner(area);
        block.render(area, buf);

        Paragraph::new(Line::from(self.label)).style(style).centered().render(inner, buf);
    }
}
