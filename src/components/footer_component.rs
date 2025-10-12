use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::symbols::line::{BOTTOM_LEFT, BOTTOM_RIGHT};
use ratatui::text::{Line, Span};

use crate::action::Action;
use crate::components::{Component, ComponentId};
use crate::widgets::shortcut::Shortcut;

pub struct FooterComponent {
    shortcuts: Vec<Shortcut>,
}

fn default_shortcuts() -> Vec<Shortcut> {
    vec![Shortcut::from("help", 0).unwrap(), Shortcut::from("quit", 0).unwrap()]
}

impl Default for FooterComponent {
    fn default() -> Self {
        Self { shortcuts: default_shortcuts() }
    }
}

impl FooterComponent {
    fn short_cuts_widget(&self) -> Line<'_> {
        let mut spans = vec![];
        for shortcut in &self.shortcuts {
            spans.push(Span::raw(BOTTOM_RIGHT));
            spans.extend(shortcut.spans(None));
            spans.push(Span::raw(BOTTOM_LEFT));
        }

        Line::from(spans)
    }
}

impl Component for FooterComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Footer
    }

    fn update(&mut self, action: Action) -> anyhow::Result<Option<Action>> {
        if let Action::Shortcuts(shortcuts) = action {
            let mut sc = default_shortcuts();
            sc.extend(shortcuts);
            self.shortcuts = sc;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> anyhow::Result<()> {
        // NOTE: bottom border may not need to be cleared, because it does not change background
        // color or other special styles frame.render_widget(Clear, area);
        frame.render_widget(self.short_cuts_widget(), area);
        Ok(())
    }
}
