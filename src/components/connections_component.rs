use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use color_eyre::Result;
use color_eyre::eyre::OptionExt;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::prelude::Line;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols::line;
use ratatui::text::Span;
use ratatui::widgets::{
    Block, BorderType, Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState,
};
use throbber_widgets_tui::{Throbber, ThrobberState};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::action::Action;
use crate::api::Api;
use crate::components::connections::{CONNECTION_COLS, Connections};
use crate::components::shortcut::{Fragment, Shortcut};
use crate::components::state::SearchState;
use crate::components::{Component, ComponentId};
use crate::models::Connection;
use crate::models::sort::SortDir;
use crate::utils::symbols::{arrow, triangle};
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};

const ROW_HEIGHT: usize = 1;

#[derive(Default)]
pub struct ConnectionsComponent {
    token: CancellationToken,
    conns_rx: Option<Receiver<Vec<Connection>>>,
    store: Arc<Connections>,
    search_state: Arc<Mutex<SearchState>>,
    live_mode: Arc<AtomicBool>,

    viewport: u16,
    item_size: usize,
    table_state: TableState,
    scroll_state: ScrollbarState,
    throbber_state: ThrobberState,
    action_tx: Option<UnboundedSender<Action>>,
}

impl ConnectionsComponent {
    pub fn new(conns_rx: Receiver<Vec<Connection>>) -> Self {
        let mut component = Self::default();
        component.conns_rx = Some(conns_rx);
        component.search_state = Arc::new(Mutex::new(SearchState::new(CONNECTION_COLS.len())));
        component.live_mode = Arc::new(AtomicBool::new(true));

        component
    }

    fn loader_connections(&mut self) -> Result<()> {
        let store = Arc::clone(&self.store);
        let search_state = Arc::clone(&self.search_state);
        let live_mode = Arc::clone(&self.live_mode);

        let mut rx = self
            .conns_rx
            .as_ref()
            .ok_or_eyre("`ConnectionsComponent` expects a Receiver<Vec<Connection>>")?
            .resubscribe();
        let token = self.token.clone();
        tokio::task::Builder::new().name("connections-loader").spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    res = rx.recv() => match res {
                        Ok(records) => {
                            store.push(false, records);
                            if live_mode.load(Ordering::Relaxed) {
                                let search_state = search_state.lock().unwrap().clone();
                                store.compute_view(&search_state);
                            }
                        },
                        Err(RecvError::Lagged(_)) => continue,
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        })?;

        Ok(())
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        let records = self.store.view();
        self.item_size = records.len();
        self.scroll_state = self.scroll_state.content_length(self.item_size * ROW_HEIGHT);
        self.viewport = area.height.saturating_sub(2); // borders

        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw("connections ("),
            Span::styled(self.item_size.to_string(), Color::Cyan),
            Span::raw(")"),
            Span::raw(TOP_TITLE_RIGHT),
        ]);
        let block = Block::bordered().border_type(BorderType::Rounded).title(title_line);
        let sort = self.search_state.lock().unwrap().sort;
        let header = CONNECTION_COLS
            .iter()
            .map(|def| def.title)
            .enumerate()
            .map(|(index, title)| {
                if let Some(sort) = sort
                    && index == sort.col
                {
                    let arrow = match sort.dir {
                        SortDir::Asc => triangle::UP,
                        SortDir::Desc => triangle::DOWN,
                    };
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
                Constraint::Max(20),
            ],
        )
        .block(block)
        .header(header)
        .column_spacing(2)
        .row_highlight_style(selected_row_style);

        frame.render_stateful_widget(table, area, &mut self.table_state);

        let (throbber_label, throbber_color) = if self.live_mode.load(Ordering::Relaxed) {
            ("Live  ", Color::Green)
        } else {
            ("Paused", Color::Red)
        };
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
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .track_symbol(Some(line::VERTICAL))
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

    fn live_mode(&mut self, live_mode: bool) {
        self.live_mode.store(live_mode, Ordering::Relaxed);
        if live_mode {
            self.table_state.select(None);
            self.scroll_state = self.scroll_state.position(0);
        }
    }

    fn handle_search_state_changed(&self, state: &SearchState) {
        // recompute view only when not in live mode, and has sorting specified
        if !self.live_mode.load(Ordering::Relaxed)
            && let Some(_) = state.sort
        {
            self.store.compute_view(state);
        }
    }
}

impl Drop for ConnectionsComponent {
    fn drop(&mut self) {
        self.token.cancel();
        info!("`ConnectionsComponent` dropped, background task cancelled");
    }
}

impl Component for ConnectionsComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Connections
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::hl(arrow::UP),
                Fragment::raw(" select "),
                Fragment::hl(arrow::DOWN),
            ]),
            Shortcut::new(vec![Fragment::raw("first "), Fragment::hl("g")]),
            Shortcut::new(vec![Fragment::raw("last "), Fragment::hl("G")]),
            Shortcut::new(vec![
                Fragment::hl(arrow::LEFT),
                Fragment::raw(" sort "),
                Fragment::hl(arrow::RIGHT),
            ]),
            Shortcut::from("reverse", 0).unwrap(),
            Shortcut::from("terminal", 0).unwrap(),
            Shortcut::new(vec![Fragment::raw("detail "), Fragment::hl("↵")]),
            Shortcut::new(vec![Fragment::raw("live "), Fragment::hl("Esc")]),
        ]
    }

    fn init(&mut self, _api: Arc<Api>) -> Result<()> {
        self.token = CancellationToken::new();
        self.loader_connections()?;
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Esc => self.live_mode(true),
            KeyCode::Char('g') => {
                self.first_row();
                self.live_mode(false);
            }
            KeyCode::Char('G') => {
                self.last_row();
                self.live_mode(false);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.next_row();
                self.live_mode(false);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.prev_row();
                self.live_mode(false);
            }
            KeyCode::Char('h') | KeyCode::Left => {
                let mut guard = self.search_state.lock().unwrap();
                guard.sort_prev();
                self.handle_search_state_changed(&guard.clone());
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let mut guard = self.search_state.lock().unwrap();
                guard.sort_next();
                self.handle_search_state_changed(&guard.clone());
            }
            KeyCode::Char('r') => {
                let mut guard = self.search_state.lock().unwrap();
                guard.sort_rev();
                self.handle_search_state_changed(&guard.clone());
            }
            KeyCode::Char('f') => return Ok(Some(Action::Focus(ComponentId::Search))),
            KeyCode::Enter => {
                let action = self
                    .table_state
                    .selected()
                    .and_then(|idx| self.store.get(idx))
                    .map(Action::ConnectionDetail);
                return Ok(action);
            }
            _ => (),
        };

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Quit => self.token.cancel(),
            Action::Tick => {
                if self.live_mode.load(Ordering::Relaxed) {
                    self.throbber_state.calc_next();
                }
            }
            Action::SearchInputChanged(pattern) => {
                self.search_state.lock().unwrap().pattern = pattern;
            }
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render_table(frame, area);
        self.render_scrollbar(frame, area);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_navigation() {
        let mut component = ConnectionsComponent::default();
        component.item_size = 3;
        assert_eq!(component.table_state.selected(), None);

        // Test next
        let next_rows_case = vec![Some(0), Some(1), Some(2)];
        for expected in next_rows_case {
            component.next_row();
            assert_eq!(component.table_state.selected(), expected);
        }

        // Test prev
        let prev_rows_case = vec![Some(1), Some(0), Some(2), Some(1)];
        for expected in prev_rows_case {
            component.prev_row();
            assert_eq!(component.table_state.selected(), expected);
        }
    }

    #[test]
    fn test_fuzzy_match() {
        use fuzzy_matcher::FuzzyMatcher;
        use fuzzy_matcher::skim::SkimMatcherV2;

        let matcher = SkimMatcherV2::default();
        let text = "nginx: worker process";

        let pattern = "nginx";
        let score = matcher.fuzzy_match(text, pattern);
        assert!(score.is_some());
        println!("Score: {:?}", score);

        let pattern = "wrk";
        let score = matcher.fuzzy_match(text, pattern);
        assert!(score.is_some());
        println!("Score: {:?}", score);

        let pattern = "apache";
        let score = matcher.fuzzy_match(text, pattern);
        assert!(score.is_none());
        println!("Score: {:?}", score);

        let pattern = "krw";
        let score = matcher.fuzzy_match(text, pattern);
        assert!(score.is_none());
        println!("Score: {:?}", score);
    }
}
