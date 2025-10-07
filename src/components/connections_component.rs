use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::prelude::Line;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Cell, Row, Table, TableState};
use throbber_widgets_tui::{Throbber, ThrobberState};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::action::Action;
use crate::api::Api;
use crate::components::connections::{CONNECTION_COLS, Connections};
use crate::components::state::SearchState;
use crate::components::{Component, ComponentId};
use crate::models::Connection;
use crate::models::sort::SortDir;
use crate::utils::symbols::{arrow, triangle};
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const ROW_HEIGHT: usize = 1;

#[derive(Default)]
pub struct ConnectionsComponent {
    token: CancellationToken,
    conns_rx: Option<Receiver<Vec<Connection>>>,
    store: Arc<Connections>,
    search_state: Arc<Mutex<SearchState>>,
    live_mode: Arc<AtomicBool>,

    table_state: TableState,
    navigator: ScrollableNavigator,
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
            .ok_or_else(|| anyhow!("`ConnectionsComponent` expects a Receiver<Vec<Connection>>"))?
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
        let len = records.len();
        // update scroller, viewport = area.height - 2 (border) - 2 (table header)
        self.navigator.length(len, (area.height - 2 - 2) as usize);

        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw("connections ("),
            Span::styled(
                self.navigator.focused.map(|i| (i + 1).to_string()).unwrap_or("-".into()),
                Color::LightCyan,
            ),
            Span::raw("/"),
            Span::styled(self.navigator.scroller.content_length().to_string(), Color::Cyan),
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

        let visible = &records[self.navigator.scroller.pos()..self.navigator.scroller.end_pos()];
        let rows: Vec<Row> = visible
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

        *self.table_state.selected_mut() =
            self.navigator.focused.map(|v| v.saturating_sub(self.navigator.scroller.pos()));
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

    fn live_mode(&mut self, live_mode: bool) {
        self.live_mode.store(live_mode, Ordering::Relaxed);
        if live_mode {
            self.navigator.focused = None;
            self.navigator.scroller.position(0);
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
        if self.navigator.handle_key_event(false, key) {
            self.live_mode(false);
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc => self.live_mode(true),
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
            KeyCode::Char('t') => {
                let action = self
                    .table_state
                    .selected()
                    .and_then(|idx| self.store.get(idx))
                    .map(Action::ConnectionTerminateRequest);
                return Ok(action);
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
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
