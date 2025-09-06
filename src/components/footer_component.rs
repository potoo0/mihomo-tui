use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Stylize};
use ratatui::text::Line;

use crate::action::Action;
use crate::components::shortcut::Shortcut;
use crate::components::{AppState, Component, ComponentId};

pub struct FooterComponent {
    shortcuts: Vec<Shortcut>,
}

fn get_default_shortcuts() -> Vec<Shortcut> {
    vec![Shortcut::new("h", "Help"), Shortcut::new("q", "Quit")]
}

impl Default for FooterComponent {
    fn default() -> Self {
        Self { shortcuts: get_default_shortcuts() }
    }
}

impl FooterComponent {
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

    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> {
        if let Action::Shortcuts(shortcuts) = action {
            let mut sc = get_default_shortcuts();
            sc.extend(shortcuts);
            self.shortcuts = sc;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, _state: &AppState) -> color_eyre::Result<()> {
        frame.render_widget(self.short_cuts_widget(), area);
        Ok(())
    }
}
