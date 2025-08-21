use crate::action::Action;
use crate::components::shortcut::Shortcut;
use crate::components::{Component, ComponentId};
use ratatui::layout::Rect;
use ratatui::style::{Color, Stylize};
use ratatui::text::Line;
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Default)]
pub struct FooterComponent {
    shortcuts: Vec<Shortcut>,
    main_component: ComponentId,
    action_tx: Option<UnboundedSender<Action>>,
}

impl FooterComponent {
    pub fn new() -> Self {
        let shortcuts = vec![Shortcut::new("h", "Help"), Shortcut::new("q", "Quit")];
        Self {
            shortcuts,
            ..Self::default()
        }
    }

    fn short_cuts_widget(&self) -> Line<'_> {
        let mut spans = vec![];
        for shortcut in &self.shortcuts {
            spans.push(format!("[{}]", shortcut.key).bold().fg(Color::DarkGray));
            spans.push(format!(":{}   ", shortcut.description).fg(Color::DarkGray));
        }

        Line::from(spans)
    }
}

impl Component for FooterComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Footer
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> eyre::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> eyre::Result<()> {
        frame.render_widget(self.short_cuts_widget(), area);
        Ok(())
    }
}
