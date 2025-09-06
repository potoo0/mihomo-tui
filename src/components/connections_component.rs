use std::sync::OnceLock;

use color_eyre::Result;
use const_format::concatcp;
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
use crate::models::search_query::OrderBy;
use crate::utils::symbols::{arrow, triangle};

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
        self.scroll_state = self.scroll_state.content_length(self.item_size * ROW_HEIGHT);

        self.viewport = area.height.saturating_sub(2); // borders
        let block = Block::bordered().border_type(BorderType::Rounded).title(Span::styled(
            format!("connections ({})", self.item_size),
            Style::default().fg(Color::Cyan),
        ));
        let header = CONNECTION_COLS
            .iter()
            .map(|def| def.title)
            .enumerate()
            .map(|(index, title)| {
                if index + 1 == self.selected_column {
                    let arrow = if self.sort_desc { triangle::DOWN } else { triangle::UP };
                    Cell::from(format!("{} {}", title, arrow)).bold().cyan()
                } else {
                    Cell::from(title).bold()
                }
            })
            .collect::<Row>()
            .height(1)
            .bottom_margin(1);
        let selected_row_style = Style::default().add_modifier(Modifier::REVERSED).fg(Color::Cyan);

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

        let (throbber_label, throbber_color) =
            if self.live_mode.0 { ("Live  ", Color::Green) } else { ("Paused", Color::Red) };
        let symbol = Throbber::default()
            .label(throbber_label)
            .style(Style::default().bg(throbber_color).bold())
            .throbber_style(Style::default().bg(throbber_color).bold())
            .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
            .use_type(throbber_widgets_tui::WhichUse::Spin);
        frame.render_stateful_widget(
            symbol,
            Rect::new(area.right().saturating_sub(9), area.y, 8, 1),
            &mut self.throbber_state,
        );
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some(arrow::UP))
                .end_symbol(Some(arrow::DOWN)),
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

    fn sortable_cols() -> &'static [usize] {
        static SORTABLE_COLS: OnceLock<Vec<usize>> = OnceLock::new();
        SORTABLE_COLS.get_or_init(|| {
            CONNECTION_COLS
                .iter()
                .enumerate()
                .filter(|(_, col)| col.sortable)
                .map(|(i, _)| i)
                .collect()
        })
    }

    /// Wrap around to the previous sortable column.
    /// If no column is selected, select the last sortable column.
    pub fn prev_column(&mut self) {
        let cols = Self::sortable_cols();
        if cols.is_empty() {
            return;
        }
        // jump to last column if no column is selected
        if self.selected_column == 0 {
            self.selected_column = cols[cols.len() - 1] + 1;
            return;
        }
        let actual_idx = self.selected_column - 1;
        let pos = cols.iter().position(|&i| i == actual_idx).unwrap_or(0);
        self.selected_column = cols[(pos + cols.len() - 1) % cols.len()] + 1
    }

    /// Wrap around to the next sortable column.
    /// If no column is selected, select the first sortable column.
    pub fn next_column(&mut self) {
        let cols = Self::sortable_cols();
        if cols.is_empty() {
            return;
        }
        // jump to first column if no column is selected
        if self.selected_column == 0 {
            self.selected_column = cols[0] + 1;
            return;
        }
        let actual_idx = self.selected_column - 1;
        let pos = cols.iter().position(|&i| i == actual_idx).unwrap_or(0);
        self.selected_column = cols[(pos + 1) % cols.len()] + 1
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
            Shortcut::new(concatcp!("j/", arrow::DOWN), "Down"),
            Shortcut::new(concatcp!("k/", arrow::UP), "Up"),
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
                self.prev_column();
                self.should_send_sort = true;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.next_column();
                self.should_send_sort = true;
            }
            KeyCode::Char('r') => {
                self.sort_desc = !self.sort_desc;
                self.should_send_sort = true;
            }
            KeyCode::Char('f') => return Ok(Some(Action::Focus(ComponentId::Search))),
            KeyCode::Enter => {
                return Ok(self.table_state.selected().map(Action::RequestConnectionDetail));
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
                    let ordering: Option<OrderBy> = if self.selected_column > 0 {
                        Some(OrderBy(self.selected_column - 1, self.sort_desc))
                    } else {
                        None
                    };
                    self.action_tx.as_ref().unwrap().send(Action::Ordering(ordering))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_navigation() {
        let mut component = ConnectionsComponent { item_size: 3, ..Default::default() };
        assert_eq!(component.table_state.selected(), None);

        // Test next_row
        let next_rows_case = vec![Some(0), Some(1), Some(2)];
        for expected in next_rows_case {
            component.next_row();
            assert_eq!(component.table_state.selected(), expected);
        }

        // Test prev_row
        let prev_rows_case = vec![Some(1), Some(0), Some(2), Some(1)];
        for expected in prev_rows_case {
            component.prev_row();
            assert_eq!(component.table_state.selected(), expected);
        }
    }

    #[test]
    fn test_column_navigation() {
        let mut component = ConnectionsComponent::default();
        assert_eq!(component.selected_column, 0);

        let cols = ConnectionsComponent::sortable_cols();
        assert!(!cols.is_empty());

        // Test next_column
        for &val in cols.iter() {
            component.next_column();
            assert_eq!(component.selected_column - 1, val);
        }
        // wrap around to first sortable column
        component.next_column();
        assert_eq!(component.selected_column, 1);

        // Reset to no column selected
        component.selected_column = 0;
        // Test prev_column
        for &val in cols.iter().rev() {
            component.prev_column();
            assert_eq!(component.selected_column - 1, val);
        }
        // wrap around to last sortable column
        component.prev_column();
        assert_eq!(component.selected_column - 1, cols[cols.len() - 1]);
    }
}
