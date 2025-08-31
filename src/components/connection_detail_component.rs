use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use serde::Serialize;
use serde_json::Serializer;
use serde_json::ser::PrettyFormatter;

use crate::action::Action;
use crate::components::shortcut::Shortcut;
use crate::components::{AppState, Component, ComponentId};
use crate::models::Connection;

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
    fn show(&mut self, data: Connection) {
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

    fn pretty(data: Connection) -> String {
        let mut buf = Vec::with_capacity(512);
        let formatter = PrettyFormatter::with_indent(INDENT);
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        if data.serialize(&mut ser).is_ok() {
            String::from_utf8(buf).unwrap_or_else(|_| "<utf8 error>".into())
        } else {
            "<invalid json>".into()
        }
    }

    fn popup_area(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
        let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        area
    }
}

impl Component for ConnectionDetailComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ConnectionDetail
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new("j|↓", "Scroll Down"),
            Shortcut::new("k|↑", "Scroll Up"),
            Shortcut::new("Space|PageDown", "Page Down"),
            Shortcut::new("PageUp", "Page Up"),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => return Ok(Some(Action::Quit)),
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll = self.scroll.saturating_add(self.viewport);
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
            self.show(connection)
        };

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, _state: &AppState) -> color_eyre::Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = Self::popup_area(area, 80, 80);
        self.viewport = area.height.saturating_sub(2) as usize;

        // content
        let para = Paragraph::new(self.data.as_str())
            .scroll((self.scroll as u16, 0))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Connection Detail"),
            );

        frame.render_widget(Clear, area); // clears out the background
        frame.render_widget(para, area);

        // scrollbar
        let sb = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        frame.render_stateful_widget(sb, area, &mut self.scroll_state);

        Ok(())
    }
}
