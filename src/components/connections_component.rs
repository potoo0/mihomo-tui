use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::prelude::Span;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::widgets::{
    Block, BorderType, Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState,
};
use throbber_widgets_tui::{Throbber, ThrobberState};
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::components::columns::CONNECTION_COLS;
use crate::components::shortcut::Shortcut;
use crate::components::{AppState, Component, ComponentId};

const ROW_HEIGHT: usize = 1;
const COLS_LEN: usize = CONNECTION_COLS.len();

#[derive(Debug, Clone, Copy)]
struct LiveMode(bool);

impl Default for LiveMode {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Debug, Default)]
pub struct ConnectionsComponent {
    should_send_sort: bool,
    sort_desc: bool,
    selected_column: usize, // starting with 1, default no sorting
    viewport: u16,
    live_mode: LiveMode,
    item_size: usize,
    table_state: TableState,
    scroll_state: ScrollbarState,
    throbber_state: ThrobberState,
    action_tx: Option<UnboundedSender<Action>>,
}

impl ConnectionsComponent {
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
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(Span::styled(
                format!("connections ({})", self.item_size),
                Style::default().fg(Color::Cyan),
            ));
        let header = CONNECTION_COLS
            .iter()
            .map(|def| def.title)
            .enumerate()
            .map(|(index, title)| {
                if index + 1 == self.selected_column {
                    let arrow = if self.sort_desc { "▿" } else { "▵" };
                    Cell::from(format!("{} {}", title, arrow)).bold().cyan()
                } else {
                    Cell::from(title).bold()
                }
            })
            .collect::<Row>()
            .height(1)
            .bottom_margin(1);
        let selected_row_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(Color::Cyan);

        // TODO: Implement virtualized rendering: only render rows within the visible viewport
        let rows: Vec<Row> = records
            .iter()
            .map(|item| {
                Row::new(CONNECTION_COLS.iter().map(|def| (def.accessor)(item)))
                    .height(ROW_HEIGHT as u16)
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Min(30),
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

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Esc => {
                self.table_state.select(None);
                self.scroll_state = self.scroll_state.position(0);
                return Ok(Some(Action::LiveMode(true)));
            }
            KeyCode::Char('g') => {
                self.first_row();
                return Ok(Some(Action::LiveMode(false)));
            }
            KeyCode::Char('G') => {
                self.last_row();
                return Ok(Some(Action::LiveMode(false)));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.next_row();
                return Ok(Some(Action::LiveMode(false)));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.prev_row();
                return Ok(Some(Action::LiveMode(false)));
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if self.selected_column == 0 {
                    self.selected_column = COLS_LEN - 1;
                } else {
                    self.selected_column -= 1;
                }
                self.should_send_sort = true;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                // todo consider skipping non-sortable columns
                if self.selected_column == COLS_LEN {
                    self.selected_column = 0;
                } else {
                    self.selected_column += 1;
                }
                self.should_send_sort = true;
            }
            KeyCode::Char('r') => {
                self.sort_desc = !self.sort_desc;
                self.should_send_sort = true;
            }
            KeyCode::Char('f') => return Ok(Some(Action::Focus(ComponentId::Search))),
            KeyCode::Enter => {
                return Ok(self
                    .table_state
                    .selected()
                    .map(Action::RequestConnectionDetail));
            }
            _ => (),
        };

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {
                if self.live_mode.0 {
                    self.throbber_state.calc_next();
                }
                if self.should_send_sort {
                    self.should_send_sort = false;
                    let ordering: Option<(usize, bool)> = if self.selected_column > 0 {
                        Some((self.selected_column - 1, self.sort_desc))
                    } else {
                        None
                    };
                    self.action_tx
                        .as_ref()
                        .unwrap()
                        .send(Action::Ordering(ordering))?;
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
        self.render_table(frame, area, state);
        self.render_scrollbar(frame, area);

        Ok(())
    }
}
