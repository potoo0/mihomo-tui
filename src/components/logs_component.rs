use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::components::{AppState, Component, ComponentId};

#[derive(Debug, Default)]
pub struct LogsComponent {}

impl Component for LogsComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Logs
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, state: &AppState) -> color_eyre::Result<()> {
        let outer_block = Block::bordered().border_type(BorderType::Rounded);
        frame.render_widget(outer_block, area);

        let span = Span::styled("Log", Style::new());
        let paragraph = Paragraph::new(span).centered();
        frame.render_widget(paragraph, area);
        Ok(())
    }
}
