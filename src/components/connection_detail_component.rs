use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Style;
use ratatui::style::Color;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use serde::Serialize;
use serde_json::Serializer;
use serde_json::ser::PrettyFormatter;

use crate::action::Action;
use crate::components::shortcut::{Fragment, Shortcut};
use crate::components::{Component, ComponentId};
use crate::models::Connection;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{popup_area, top_title_line};
use crate::widgets::scrollbar::Scroller;

const INDENT: &[u8; 4] = b"    "; // 4 spaces

#[derive(Debug, Default)]
pub struct ConnectionDetailComponent {
    show: bool,
    total_lines: usize,
    data: String,

    scroller: Scroller,
}

impl ConnectionDetailComponent {
    fn show(&mut self, data: &Connection) {
        self.show = true;

        let pretty = Self::pretty(data);
        self.total_lines = pretty.lines().count();
        self.data = pretty;
        self.scroller.position(0);
    }

    fn hide(&mut self) {
        self.show = false;
        self.data = String::default();
    }

    fn pretty(data: &Connection) -> String {
        let mut buf = Vec::with_capacity(512);
        let formatter = PrettyFormatter::with_indent(INDENT);
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        if data.serialize(&mut ser).is_ok() {
            String::from_utf8(buf).unwrap_or_else(|_| "<utf8 error>".into())
        } else {
            "<invalid json>".into()
        }
    }
}

impl Component for ConnectionDetailComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ConnectionDetail
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::raw("esc "),
                Fragment::hl("Esc"),
                Fragment::raw("/"),
                Fragment::hl("Enter"),
            ]),
            Shortcut::new(vec![
                Fragment::hl(arrow::UP),
                Fragment::raw(" scroll "),
                Fragment::hl(arrow::DOWN),
            ]),
            Shortcut::new(vec![
                Fragment::hl("Space/PgDn"),
                Fragment::raw(" page "),
                Fragment::hl("PgUp"),
            ]),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.scroller.handle_key_event(key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            _ => {}
        };
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if let Action::ConnectionDetail(connection) = action {
            self.show(connection.as_ref())
        };

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = popup_area(area, 80, 75);
        self.scroller.length(self.total_lines, area.height.saturating_sub(2) as usize);

        // content
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("detail", Style::default()));
        let paragraph =
            Paragraph::new(self.data.as_str()).scroll((self.scroller.pos() as u16, 0)).block(block);

        frame.render_widget(Clear, area); // clears out the background
        frame.render_widget(paragraph, area);

        self.scroller.render(frame, area);

        Ok(())
    }
}
