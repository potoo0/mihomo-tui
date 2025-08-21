use color_eyre::Result;
use crate::action::Action;
use crate::components::{Component, ComponentId};
use ratatui::{
    Frame,
    layout::{Rect},
};
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};

#[derive(Debug, Default)]
pub struct OverviewComponent {}

impl OverviewComponent {
    pub fn new() -> Self {
        Self {}
    }
}

impl Component for OverviewComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Overview
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        // match action {
        //     Action::Tick => debug!("OverviewComponent ticked"),
        //     Action::Render => debug!("OverviewComponent rendered"),
        //     _ => {}
        // };
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let outer_block = Block::default().borders(Borders::ALL);
        frame.render_widget(outer_block, area);

        let span = Span::styled("Overview", Style::new());
        let paragraph = Paragraph::new(span).centered();
        frame.render_widget(paragraph, area);
        Ok(())
    }
}
