use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Style;
use ratatui::style::Color;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use crate::app::action::Action;
use crate::components::{Component, ComponentContext, PopupComponent, PopupId};
use crate::models::proxy::Proxy;
use crate::ui::layout::{popup_area, top_title_line};
use crate::ui::scrollbar::Scroller;
use crate::ui::shortcut::{Fragment, Shortcut};
use crate::ui::symbols::arrow;
use crate::utils::json::pretty_json;

#[derive(Debug)]
pub struct ProxyInspectComponent {
    show: bool,
    total_lines: usize,
    data: String,
    scroller: Scroller,
}

impl ProxyInspectComponent {
    pub fn new(_ctx: &ComponentContext) -> Self {
        Self {
            show: Default::default(),
            total_lines: Default::default(),
            data: Default::default(),
            scroller: Default::default(),
        }
    }

    fn show(&mut self, proxy: &Proxy) {
        self.show = true;
        let pretty = pretty_json(proxy);
        self.total_lines = pretty.lines().count();
        self.data = pretty;
        self.scroller.position(0);
    }

    fn hide(&mut self) {
        self.show = false;
        self.data.clear();
        self.total_lines = 0;
    }
}

impl PopupComponent for ProxyInspectComponent {
    fn popup_id(&self) -> PopupId {
        PopupId::ProxyInspect
    }
}

impl Component for ProxyInspectComponent {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::key("i"),
                Fragment::sep("/"),
                Fragment::key("q"),
                Fragment::sep("/"),
                Fragment::key("Esc"),
                Fragment::sep("/"),
                Fragment::key("Enter"),
            ]),
            Shortcut::new(vec![
                Fragment::key(arrow::UP),
                Fragment::label("shortcuts.scroll"),
                Fragment::key(arrow::DOWN),
            ]),
            Shortcut::new(vec![
                Fragment::key("PgUp"),
                Fragment::label("shortcuts.page"),
                Fragment::key("PgDn"),
            ]),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.scroller.handle_key_event(key).is_consumed() {
            return Ok(None);
        }
        if matches!(
            key.code,
            KeyCode::Char('i') | KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter
        ) {
            self.hide();
            return Ok(Some(self.close_action()));
        }
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if let Action::ProxyInspect(proxy) = action {
            self.show(proxy.as_ref());
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = popup_area(area, 80, 75);
        self.scroller.length(self.total_lines, area.height.saturating_sub(2) as usize);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("titles.inspect", Style::default()));
        let paragraph =
            Paragraph::new(self.data.as_str()).scroll((self.scroller.pos() as u16, 0)).block(block);

        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
        self.scroller.render(frame, area);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crossterm::event::KeyModifiers;
    use serde_json::json;

    use super::*;
    use crate::components::FocusTarget;

    fn proxy() -> Proxy {
        serde_json::from_value(json!({
            "name": "node",
            "type": "Shadowsocks",
            "history": [],
            "udp": true
        }))
        .unwrap()
    }

    #[test]
    fn renders_proxy_json_and_closes() {
        for close_key in [KeyCode::Char('i'), KeyCode::Char('q'), KeyCode::Esc, KeyCode::Enter] {
            let mut component = ProxyInspectComponent::new(&ComponentContext::test_ctx());
            component.update(Action::ProxyInspect(Arc::new(proxy()))).unwrap();

            assert!(component.show);
            assert!(component.data.contains("\"udp\": true"));

            let action =
                component.handle_key_event(KeyEvent::new(close_key, KeyModifiers::NONE)).unwrap();
            assert!(matches!(
                action,
                Some(Action::Unfocus(FocusTarget::Popup(PopupId::ProxyInspect)))
            ));
            assert!(!component.show);
        }
    }
}
