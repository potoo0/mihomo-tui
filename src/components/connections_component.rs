use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::prelude::Span;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState,
};
use throbber_widgets_tui::{Throbber, ThrobberState};

use crate::action::Action;
use crate::components::shortcut::Shortcut;
use crate::components::{AppState, Component, ComponentId};
use crate::utils::byte_size::human_bytes;

const ROW_HEIGHT: usize = 1;

#[derive(Debug, Clone, Copy)]
struct LiveMode(bool);

impl Default for LiveMode {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Debug, Default)]
pub struct ConnectionsComponent {
    viewport: u16,
    live_mode: LiveMode,
    item_size: usize,
    table_state: TableState,
    scroll_state: ScrollbarState,
    throbber_state: ThrobberState,
}

impl ConnectionsComponent {
    fn render_header(&mut self, frame: &mut Frame, area: Rect, _state: &AppState) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Filter", Color::Cyan));
        frame.render_widget(block, area);
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect, state: &AppState) {
        let records = {
            let conns = state.connections.lock().unwrap();
            conns.to_vec()
        };
        self.item_size = records.len();
        self.scroll_state = self
            .scroll_state
            .content_length(self.item_size * ROW_HEIGHT);

        self.viewport = area.height.saturating_sub(2); // borders
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(Span::styled(
                format!("Connections ({})", self.item_size),
                Style::default().fg(Color::Cyan),
            ));
        let header = [
            "Host",
            "Rule",
            "Chains",
            "DownRate",
            "UpRate",
            "DownTotal",
            "UpTotal",
            "SourceIP",
        ]
        .into_iter()
        .map(|v| Cell::from(v).bold())
        .collect::<Row>()
        .height(1)
        .bottom_margin(1);
        let selected_row_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(Color::Cyan);

        // TODO: Implement virtualized rendering: only render rows within the visible viewport
        let rows = records.iter().map(|item| {
            let cells = vec![
                Cell::from(item.metadata.host.as_str()),
                Cell::from(item.rule.as_str()),
                Cell::from(item.chains.join(" > ")),
                Cell::from("-"), // todo: calculate rate
                Cell::from("-"),
                Cell::from(human_bytes(item.download as f64, None)),
                Cell::from(human_bytes(item.upload as f64, None)),
                Cell::from(item.metadata.source_ip.as_str()),
            ];
            Row::new(cells).height(ROW_HEIGHT as u16)
        });
        let host_width = records
            .iter()
            .map(|item| item.metadata.host.len())
            .max()
            .unwrap_or(0);
        let table = Table::new(
            rows,
            [
                Constraint::Length(host_width as u16),
                Constraint::Max(15),
                Constraint::Min(10),
                Constraint::Max(15),
                Constraint::Max(15),
                Constraint::Max(15),
                Constraint::Max(15),
                Constraint::Max(15),
            ],
        )
        .block(block)
        .header(header)
        .column_spacing(2)
        .row_highlight_style(selected_row_style);

        frame.render_stateful_widget(table, area, &mut self.table_state);

        if self.live_mode.0 {
            let full = Throbber::default()
                .label("Live")
                .style(Style::default().bg(Color::Green).bold())
                .throbber_style(Style::default().bg(Color::Green).bold())
                .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                .use_type(throbber_widgets_tui::WhichUse::Spin);
            frame.render_stateful_widget(
                full,
                Rect::new(area.right().saturating_sub(7), area.y, 6, 1),
                &mut self.throbber_state,
            );
        }
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area.inner(Margin::new(1, 1)),
            &mut self.scroll_state,
        );
    }

    pub fn next_row(&mut self) {
        if self.item_size == 0 {
            return;
        }
        let i = self
            .table_state
            .selected()
            .map_or(0, |i| if i + 1 >= self.item_size { 0 } else { i + 1 });
        self.table_state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ROW_HEIGHT);
    }

    pub fn prev_row(&mut self) {
        if self.item_size == 0 {
            return;
        }
        let i = self
            .table_state
            .selected()
            .map_or(0, |i| if i == 0 { self.item_size - 1 } else { i - 1 });
        self.table_state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ROW_HEIGHT);
    }

    pub fn first_row(&mut self) {
        if self.item_size == 0 {
            return;
        }
        self.table_state.select(Some(0));
        self.scroll_state = self.scroll_state.position(0);
    }

    pub fn last_row(&mut self) {
        if self.item_size == 0 {
            return;
        }
        let i = self.item_size - 1;
        self.table_state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ROW_HEIGHT);
    }
}

impl Component for ConnectionsComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Connections
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new("↵", "Detail"),
            Shortcut::new("g", "First"),
            Shortcut::new("G", "Last"),
            Shortcut::new("j/↓", "Down"),
            Shortcut::new("k/↑", "Up"),
            Shortcut::new("Esc", "Live Mode"),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // TODO handle G, g, j, k, ...
        let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc => {
                self.table_state.select(None);
                self.scroll_state = self.scroll_state.position(0);
                Ok(Some(Action::LiveMode(true)))
            }
            KeyCode::Char('g') => {
                self.first_row();
                Ok(Some(Action::LiveMode(false)))
            }
            KeyCode::Char('G') => {
                self.last_row();
                Ok(Some(Action::LiveMode(false)))
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.next_row();
                Ok(Some(Action::LiveMode(false)))
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.prev_row();
                Ok(Some(Action::LiveMode(false)))
            }
            KeyCode::Enter => Ok(self
                .table_state
                .selected()
                .map(Action::RequestConnectionDetail)),
            _ => Ok(None),
        }
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {
                if self.live_mode.0 {
                    self.throbber_state.calc_next();
                }
            }
            Action::LiveMode(live) => {
                self.live_mode.0 = live;
            }
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, state: &AppState) -> Result<()> {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(0)])
            .split(area);

        self.render_header(frame, chunks[0], state);
        self.render_table(frame, chunks[1], state);
        self.render_scrollbar(frame, chunks[1]);

        Ok(())
    }
}
