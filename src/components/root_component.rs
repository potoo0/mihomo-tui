use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use color_eyre::owo_colors::OwoColorize;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph};
use ratatui::{Frame, layout::Rect};
use tracing::debug;

pub(crate) struct RootComponent {}

impl RootComponent {
    pub fn new() -> Self {
        Self {}
    }
}

impl Component for RootComponent {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        // match action {
        //     Action::Tick => debug!("OverviewComponent ticked"),
        //     Action::Render => debug!("OverviewComponent rendered"),
        //     _ => {}
        // };
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        let inner_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(outer_layout[1]);
        frame.render_widget(
            Paragraph::new("Outer 0").block(Block::bordered()),
            outer_layout[0],
        );
        frame.render_widget(
            Paragraph::new("Inner 0").block(Block::bordered()),
            inner_layout[0],
        );
        frame.render_widget(
            Paragraph::new("Inner 1").block(Block::bordered()),
            inner_layout[1],
        );
        Ok(())
    }
}
