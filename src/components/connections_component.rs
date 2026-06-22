use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Row, Table, TableState};
use ringbuffer::RingBuffer;
use throbber_widgets_tui::{BRAILLE_SIX, CANADIAN, Throbber, ThrobberState, WhichUse};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc::{Receiver, UnboundedSender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId};
use crate::models::Connection;
use crate::models::sort::SortDir;
use crate::store::connections::{CONNECTION_COLS, Connections, SourceIpAliasTextResolver};
use crate::store::connections_setting::ConnectionsSetting;
use crate::utils::columns::{TextResolver, filter_placeholder};
use crate::utils::symbols::{arrow, triangle};
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const ROW_HEIGHT: usize = 1;

pub struct ConnectionsComponent {
    token: CancellationToken,
    conns_rx: Arc<AsyncMutex<Receiver<Vec<Connection>>>>,
    action_tx: Option<UnboundedSender<Action>>,

    store: Arc<Connections>,
    navigator: ScrollableNavigator,
    table_state: TableState,

    live_mode: Arc<AtomicBool>,
    live_throbber: ThrobberState,

    capture_mode: Arc<AtomicBool>,
    capture_throbber: ThrobberState,
}

impl ConnectionsComponent {
    pub fn new(
        conns_rx: Arc<AsyncMutex<Receiver<Vec<Connection>>>>,
        store_capacity: NonZeroUsize,
    ) -> Self {
        Self {
            token: CancellationToken::new(),
            conns_rx,
            action_tx: None,
            store: Arc::new(Connections::new(store_capacity)),
            navigator: Default::default(),
            table_state: Default::default(),
            live_mode: Arc::new(AtomicBool::new(true)),
            live_throbber: Default::default(),
            capture_mode: Default::default(),
            capture_throbber: Default::default(),
        }
    }

    fn load_connections(&mut self) -> Result<()> {
        let store = Arc::clone(&self.store);
        let live_mode = Arc::clone(&self.live_mode);
        let capture_mode = Arc::clone(&self.capture_mode);
        let rx = Arc::clone(&self.conns_rx);

        let token = self.token.clone();
        tokio::task::Builder::new().name("connections-loader").spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    res = async { rx.lock().await.recv().await } => match res {
                        Some(records) => {
                            store.push(capture_mode.load(Ordering::Relaxed), records);
                            if live_mode.load(Ordering::Relaxed) {
                                store.compute_view();
                            }
                        },
                        _ => break,
                    }
                }
            }
        })?;

        Ok(())
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if self.capture_mode.load(Ordering::Relaxed) {
            let symbol = Throbber::default()
                .label("Capture")
                .style(Style::default().fg(Color::White).bg(Color::Blue).bold())
                .throbber_style(Style::default().fg(Color::White).bg(Color::Blue).bold())
                .throbber_set(CANADIAN)
                .use_type(WhichUse::Full);
            frame.render_stateful_widget(
                symbol,
                Rect::new(area.right().saturating_sub(20), area.y, 9, 1),
                &mut self.capture_throbber,
            );
        }
        let (throbber_label, throbber_color) = if self.live_mode.load(Ordering::Relaxed) {
            ("Live  ", Color::Green)
        } else {
            ("Paused", Color::Red)
        };
        let symbol = Throbber::default()
            .label(throbber_label)
            .style(Style::default().bg(throbber_color).bold())
            .throbber_style(Style::default().bg(throbber_color).bold())
            .throbber_set(BRAILLE_SIX)
            .use_type(WhichUse::Spin);
        frame.render_stateful_widget(
            symbol,
            Rect::new(area.right().saturating_sub(9), area.y, 8, 1),
            &mut self.live_throbber,
        );
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        let records = self.store.with_view(|records| {
            // update scroller, viewport = area.height - 2 (border) - 2 (table header)
            self.navigator.length(records.len(), (area.height - 2 - 2) as usize);
            // NOTE: end_pos() depends on length()
            let start = self.navigator.scroller.pos();
            let end = self.navigator.scroller.end_pos();
            records.iter().skip(start).take(end - start).cloned().collect::<Vec<_>>()
        });

        // update table selected, which is relative position in current viewport
        *self.table_state.selected_mut() =
            self.navigator.focused.map(|v| v.saturating_sub(self.navigator.scroller.pos()));

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
        let setting = ConnectionsSetting::snapshot();
        let sort = setting.query_state.sort;
        let header = setting
            .columns
            .iter()
            .filter_map(|&index| CONNECTION_COLS.get(index).map(|def| (index, def.col.title)))
            .enumerate()
            .map(|(visible_index, (_index, title))| {
                if let Some(sort) = sort
                    && visible_index == sort.col
                {
                    let arrow = match sort.dir {
                        SortDir::Asc => triangle::UP,
                        SortDir::Desc => triangle::DOWN,
                    };
                    Cell::from(format!("{}{}", title, arrow)).bold().cyan()
                } else {
                    Cell::from(title).bold()
                }
            })
            .collect::<Row>()
            .height(1)
            .bottom_margin(1);
        let selected_row_style = Style::default().add_modifier(Modifier::REVERSED).fg(Color::Cyan);
        let text_resolver = SourceIpAliasTextResolver { source_ip_alias: &setting.source_ip_alias };

        let rows: Vec<Row> =
            records
                .iter()
                .map(|item| {
                    Row::new(
                        setting.columns.iter().filter_map(|&index| CONNECTION_COLS.get(index)).map(
                            |def| text_resolver.resolve(&def.col, item, (def.col.accessor)(item)),
                        ),
                    )
                    .height(ROW_HEIGHT as u16)
                })
                .collect();
        let table = Table::new(rows, self.table_constraints(&setting))
            .block(block)
            .header(header)
            .column_spacing(2)
            .row_highlight_style(selected_row_style);

        frame.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn table_constraints(&self, setting: &ConnectionsSetting) -> Vec<Constraint> {
        let hidden_alive = !self.capture_mode.load(Ordering::Relaxed);
        let mut constraints: Vec<_> = setting
            .columns
            .iter()
            .filter_map(|&index| {
                if index == 0 && hidden_alive {
                    Some(Constraint::Length(0))
                } else {
                    Some(CONNECTION_COLS.get(index)?.constraint)
                }
            })
            .collect();

        // When the leading alive column is hidden,
        // the next visible column becomes the first layout segment.
        // A leading `Max` constraint has no lower bound and may be solved to width 0,
        // so turn it into `Length(max)` to keep it visible.
        if let [Constraint::Length(0), second, ..] = constraints.as_mut_slice()
            && let Constraint::Max(max) = *second
        {
            *second = Constraint::Length(max);
        }

        constraints
    }

    fn live_mode(&mut self, live_mode: bool) {
        self.live_mode.store(live_mode, Ordering::Relaxed);
        if live_mode {
            self.navigator.focused = None;
            self.navigator.scroller.position(0);
        }
    }

    fn handle_query_state_changed(&self) {
        // recompute view only when not in live mode, and has sorting specified
        if !self.live_mode.load(Ordering::Relaxed) {
            self.store.compute_view();
        }
        if let Some(tx) = &self.action_tx {
            let _ = tx.send(Action::ConnectionsSettingChanged);
        }
    }

    fn filter_placeholder() -> Option<String> {
        let setting = ConnectionsSetting::snapshot();
        filter_placeholder(
            setting.columns.iter().filter_map(|&idx| CONNECTION_COLS.get(idx)).map(|col| &col.col),
        )
    }

    fn filtered_active_connection_ids(&self) -> Vec<String> {
        self.store.with_view(|records| {
            records
                .iter()
                .filter(|conn| !conn.inactive.load(Ordering::Relaxed))
                .map(|conn| conn.id.clone())
                .collect()
        })
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
                Fragment::raw("/"),
                Fragment::hl("PgUp"),
                Fragment::raw("/"),
                Fragment::hl("g"),
                Fragment::raw(" nav "),
                Fragment::hl("G"),
                Fragment::raw("/"),
                Fragment::hl("PgDn"),
                Fragment::raw("/"),
                Fragment::hl(arrow::DOWN),
            ]),
            Shortcut::new(vec![
                Fragment::hl(arrow::LEFT),
                Fragment::raw(" sort "),
                Fragment::hl(arrow::RIGHT),
            ]),
            Shortcut::from("reverse", 0).unwrap(),
            Shortcut::new(vec![
                Fragment::hl("t"),
                Fragment::raw("/"),
                Fragment::hl("T"),
                Fragment::raw("erm"),
            ]),
            Shortcut::from("capture", 0).unwrap(),
            Shortcut::new(vec![Fragment::raw("detail "), Fragment::hl("↵")]),
            Shortcut::new(vec![Fragment::raw("live "), Fragment::hl("Esc")]),
            Shortcut::from("setting", 0).unwrap(),
        ]
    }

    fn init(&mut self, _api: Arc<Api>) -> Result<()> {
        self.token = CancellationToken::new();
        self.load_connections()?;
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.navigator.handle_key_event(false, key).is_consumed() {
            self.live_mode(false);
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc => self.live_mode(true),
            KeyCode::Left => {
                ConnectionsSetting::update(|setting| setting.query_state.sort_prev());
                self.handle_query_state_changed();
            }
            KeyCode::Right => {
                ConnectionsSetting::update(|setting| {
                    // When capture mode is off, the runtime Alive column is hidden with zero width.
                    // If sorting starts from None, advance once more so Right lands on the first
                    // visible user column instead of the hidden Alive column.
                    if setting.query_state.sort.is_none()
                        && !self.capture_mode.load(Ordering::Relaxed)
                    {
                        setting.query_state.sort_next()
                    }
                    setting.query_state.sort_next()
                });
                self.handle_query_state_changed();
            }
            KeyCode::Char('r') => {
                ConnectionsSetting::update(|setting| setting.query_state.sort_rev());
                self.handle_query_state_changed();
            }
            KeyCode::Char('t') => {
                let action = self
                    .navigator
                    .focused
                    .and_then(|idx| self.store.get(idx))
                    .map(Action::ConnectionTerminateRequest);
                return Ok(action);
            }
            KeyCode::Char('T') => {
                let ids = self.filtered_active_connection_ids();
                if ids.is_empty() {
                    debug!("No active filtered connections to terminate");
                    return Ok(None);
                }
                return Ok(Some(Action::ConnectionBatchTerminateRequest(ids)));
            }
            KeyCode::Char('c') => self
                .capture_mode
                .store(!self.capture_mode.load(Ordering::Relaxed), Ordering::Relaxed),
            KeyCode::Char('f') => return Ok(Some(Action::Focus(ComponentId::Filter))),
            KeyCode::Enter => {
                let action = self
                    .navigator
                    .focused
                    .and_then(|idx| self.store.get(idx))
                    .map(Action::ConnectionDetail);
                return Ok(action);
            }
            KeyCode::Char('s') => {
                return Ok(Some(Action::ConnectionsSetting(self.store.source_ips())));
            }
            _ => (),
        };

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Quit => self.token.cancel(),
            Action::Tick if self.live_mode.load(Ordering::Relaxed) => {
                self.live_throbber.calc_next();
            }
            Action::FilterChanged(pattern) => {
                debug!("handle Action::FilterChanged, got pattern={pattern:?}");
                ConnectionsSetting::update(|setting| setting.query_state.set_pattern(pattern));
            }
            Action::TabSwitch(to) if to == self.id() => {
                let pattern = ConnectionsSetting::global()
                    .write()
                    .unwrap()
                    .query_state
                    .pattern
                    .as_ref()
                    .map(|pattern| pattern.raw().into());
                debug!("handle Action::TabSwitch, current filter pattern={pattern:?}");
                if let Some(tx) = &self.action_tx {
                    tx.send(Action::FilterPlaceholder(Self::filter_placeholder()))?;
                }
                return Ok(Some(Action::FilterSet(pattern)));
            }
            Action::ConnectionsSettingChanged => {
                self.store.compute_view();
                if let Some(tx) = &self.action_tx {
                    tx.send(Action::FilterPlaceholder(Self::filter_placeholder()))?;
                }
            }
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render_table(frame, area);
        self.render_throbber(frame, area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
