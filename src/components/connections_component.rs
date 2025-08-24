use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::{Span, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::components::shortcut::Shortcut;
use crate::components::{AppState, Component, ComponentId};

#[derive(Debug, Default)]
pub struct ConnectionsComponent {}

impl Component for ConnectionsComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Connections
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![Shortcut::new("↵", "Info")]
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, state: &AppState) -> color_eyre::Result<()> {
        let outer_block = Block::default().borders(Borders::ALL);
        frame.render_widget(outer_block, area);

        let span = Span::styled("Connections", Style::new());
        let paragraph = Paragraph::new(span).centered();
        frame.render_widget(paragraph, area);
        Ok(())
    }
}
