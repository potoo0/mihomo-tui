use color_eyre::Result;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::components::{AppState, Component, ComponentId};

#[derive(Debug, Default)]
pub struct OverviewComponent {}

impl Component for OverviewComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Overview
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, _state: &AppState) -> Result<()> {
        let outer_block = Block::default().borders(Borders::ALL);
        frame.render_widget(outer_block, area);

        let span = Span::styled("Overview", Style::new());
        let paragraph = Paragraph::new(span).centered();
        frame.render_widget(paragraph, area);
        Ok(())
    }
}
