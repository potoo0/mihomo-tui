use color_eyre::Result;
use crate::action::Action;
use crate::components::Component;
use ratatui::{
    Frame,
    layout::{Rect},
};
use tracing::debug;

pub(crate) struct OverviewComponent {}

impl OverviewComponent {
    pub fn new() -> Self {
        Self {}
    }
}

impl Component for OverviewComponent {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        // match action {
        //     Action::Tick => debug!("OverviewComponent ticked"),
        //     Action::Render => debug!("OverviewComponent rendered"),
        //     _ => {}
        // };
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // let [top, _] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        // let message = format!(
        //     "{:.2} ticks/sec, {:.2} FPS",
        //     self.ticks_per_second, self.frames_per_second
        // );
        // let span = Span::styled(message, Style::new().dim());
        // let paragraph = Paragraph::new(span).right_aligned();
        // frame.render_widget(paragraph, top);
        Ok(())
    }
}
