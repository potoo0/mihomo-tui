use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::symbols::line::{BOTTOM_LEFT, BOTTOM_RIGHT};
use ratatui::text::{Line, Span};

use crate::action::Action;
use crate::components::{Component, ComponentId};
use crate::widgets::shortcut::{Shortcut, ShortcutMode, shortcuts_full_width};

#[derive(Default)]
pub struct FooterComponent {
    shortcuts: Vec<Shortcut>,
    full_width: usize,
}

impl FooterComponent {
    fn short_cuts_widget(&self, width: u16) -> Line<'_> {
        let mode = if self.full_width <= width as usize {
            ShortcutMode::Full
        } else {
            ShortcutMode::Compact
        };
        let mut spans = vec![];
        for shortcut in &self.shortcuts {
            spans.push(Span::raw(BOTTOM_RIGHT));
            spans.extend(shortcut.spans_for(mode, None));
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
            self.full_width = shortcuts_full_width(&shortcuts, 2);
            self.shortcuts = shortcuts;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> anyhow::Result<()> {
        // NOTE: bottom border may not need to be cleared, because it does not change background
        // color or other special styles frame.render_widget(Clear, area);
        frame.render_widget(self.short_cuts_widget(area.width), area);
        Ok(())
    }
}
