use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Style;
use ratatui::style::Color;
use ratatui::symbols::line;
use ratatui::widgets::{
    Block, BorderType, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use serde::Serialize;
use serde_json::Serializer;
use serde_json::ser::PrettyFormatter;

use crate::action::Action;
use crate::components::shortcut::{Fragment, Shortcut};
use crate::components::{Component, ComponentId};
use crate::models::Connection;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{popup_area, top_title_line};

const INDENT: &[u8; 4] = b"    "; // 4 spaces

#[derive(Debug, Default)]
pub struct ConnectionDetailComponent {
    show: bool,
    viewport: usize,
    scroll: usize,
    total_lines: usize,
    data: String,
    scroll_state: ScrollbarState,
}

impl ConnectionDetailComponent {
    fn show(&mut self, data: &Connection) {
        self.show = true;
        self.scroll = 0;

        let pretty = Self::pretty(data);
        self.total_lines = pretty.lines().count();
        self.data = pretty;
        self.scroll_state = self.scroll_state.content_length(self.total_lines);
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
                Fragment::hl("Space/PageDown"),
                Fragment::raw(" page "),
                Fragment::hl("PageUp"),
            ]),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1).min(self.total_lines - 1);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll = self.scroll.saturating_add(self.viewport).min(self.total_lines - 1);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.viewport);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            _ => {}
        };
        Ok(None)
    }

    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> {
        if let Action::ConnectionDetail(connection) = action {
            self.show(connection.as_ref())
        };

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = popup_area(area, 80, 75);
        self.viewport = area.height.saturating_sub(2) as usize; // minus borders

        // content
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("detail", Style::default()));
        let paragraph =
            Paragraph::new(self.data.as_str()).scroll((self.scroll as u16, 0)).block(block);

        frame.render_widget(Clear, area); // clears out the background
        frame.render_widget(paragraph, area);

        // scrollbar
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .track_symbol(Some(line::VERTICAL))
            .begin_symbol(Some(arrow::UP))
            .end_symbol(Some(arrow::DOWN));
        frame.render_stateful_widget(scrollbar, area, &mut self.scroll_state);

        Ok(())
    }
}
