use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Margin, Rect};
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
use crate::store::connections::{
    ALIVE_COLUMN_INDEX, CONNECTION_COLS, Connections, SourceIpAliasTextResolver,
};
use crate::store::connections_setting::ConnectionsSetting;
use crate::utils::columns::{TextResolver, filter_placeholder};
use crate::utils::symbols::{arrow, triangle};
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const ROW_HEIGHT: usize = 1;
const COLUMN_SPACING: u16 = 2;
const TABLE_FLEX: Flex = Flex::Start;
const LAYOUT_SAVE_TICKS: u8 = 4;

pub struct ConnectionsComponent {
    token: CancellationToken,
    conns_rx: Arc<AsyncMutex<Receiver<Vec<Connection>>>>,
    action_tx: Option<UnboundedSender<Action>>,

    store: Arc<Connections>,
    navigator: ScrollableNavigator,
    table_state: TableState,
    pending_column_width_deltas: HashMap<usize, i16>,
    layout_save_ticks_remaining: u8,

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
            pending_column_width_deltas: Default::default(),
            layout_save_ticks_remaining: 0,
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
        let mut constraints = self.table_constraints(&setting);
        self.apply_pending_column_width_deltas(&mut constraints, &setting, block.inner(area));
        let table = Table::new(rows, constraints)
            .block(block)
            .header(header)
            .flex(TABLE_FLEX)
            .column_spacing(COLUMN_SPACING)
            .row_highlight_style(selected_row_style);

        frame.render_stateful_widget(table, area, &mut self.table_state);
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

// Column width adjustment and deferred persistence.
impl ConnectionsComponent {
    fn table_constraints(&self, setting: &ConnectionsSetting) -> Vec<Constraint> {
        let hidden_alive = !self.capture_mode.load(Ordering::Relaxed);
        let mut constraints: Vec<_> = setting
            .columns
            .iter()
            .filter_map(|&index| {
                if index == ALIVE_COLUMN_INDEX && hidden_alive {
                    return Some(Constraint::Length(0));
                }
                // Looking up the definition also filters invalid column indices
                let default = CONNECTION_COLS.get(index)?.constraint;
                let constraint = match setting.column_widths.get(&index) {
                    Some(&width) => Constraint::Length(width),
                    None => default,
                };
                Some(constraint)
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

    fn apply_pending_column_width_deltas(
        &mut self,
        constraints: &mut [Constraint],
        setting: &ConnectionsSetting,
        area: Rect,
    ) {
        let Some(updates) = self.resolve_pending_column_widths(setting, constraints, area) else {
            return;
        };

        for &(_, visible_index, width) in &updates {
            constraints[visible_index] = Constraint::Length(width);
        }
        ConnectionsSetting::update(|setting| {
            for &(index, _, width) in &updates {
                setting.column_widths.insert(index, width);
            }
        });
        self.schedule_layout_save();
    }

    /// Resolves pending column-width deltas into updates for the current layout.
    ///
    /// # Arguments
    ///
    /// * `setting` - The current connections setting.
    /// * `constraints` - The current table column constraints.
    /// * `area` - The inner area of the connections table.
    ///
    /// # Returns
    ///
    /// * `Option<Vec<(usize, usize, u16)>>` - The applicable column-width updates, or `None` if no
    ///   pending delta can be applied. Each tuple contains, in order:
    ///   1. The stable column index into [`CONNECTION_COLS`].
    ///   2. The visible column index in the current table.
    ///   3. The new column width.
    fn resolve_pending_column_widths(
        &mut self,
        setting: &ConnectionsSetting,
        constraints: &[Constraint],
        area: Rect,
    ) -> Option<Vec<(usize, usize, u16)>> {
        let pending = std::mem::take(&mut self.pending_column_width_deltas);
        if pending.is_empty() {
            return None;
        }

        let rendered_areas = Layout::horizontal(constraints.iter().copied())
            .flex(TABLE_FLEX)
            .spacing(COLUMN_SPACING)
            .split(area);

        let updates = pending
            .into_iter()
            .filter(|(_, delta)| *delta != 0)
            .filter_map(|(index, delta)| {
                let visible_index = setting.columns.iter().position(|&col| col == index)?;
                let rendered_width = rendered_areas.get(visible_index)?.width;
                let current = setting.column_widths.get(&index).copied().unwrap_or(rendered_width);
                let next = current.saturating_add_signed(delta).max(1);
                (next != current).then_some((index, visible_index, next))
            })
            .collect::<Vec<_>>();

        (!updates.is_empty()).then_some(updates)
    }

    fn queue_column_width_delta(&mut self, index: usize, delta: i16) {
        if delta == 0 {
            return;
        }

        match self.pending_column_width_deltas.entry(index) {
            Entry::Occupied(mut entry) => {
                let next = entry.get().saturating_add(delta);
                if next == 0 {
                    entry.remove();
                } else {
                    *entry.get_mut() = next;
                }
            }
            Entry::Vacant(entry) => _ = entry.insert(delta),
        }
    }

    /// Gets the selected column's stable index.
    ///
    /// The sort column is temporarily used as the selection.
    fn selected_column_index(setting: &ConnectionsSetting) -> Option<usize> {
        let visible_index = setting.query_state.sort?.col;
        setting.columns.get(visible_index).copied().filter(|&index| index != ALIVE_COLUMN_INDEX)
    }

    fn adjust_column_width(&mut self, delta: i16) {
        let setting = ConnectionsSetting::snapshot();
        let Some(index) = Self::selected_column_index(&setting) else {
            return;
        };

        if let Some(&current) = setting.column_widths.get(&index) {
            let pending = self.pending_column_width_deltas.remove(&index).unwrap_or_default();
            let next = current.saturating_add_signed(pending.saturating_add(delta)).max(1);
            if next == current {
                return;
            }

            ConnectionsSetting::update(|setting| {
                setting.column_widths.insert(index, next);
            });
            self.schedule_layout_save();
        } else {
            self.queue_column_width_delta(index, delta);
        }
    }

    fn reset_column_width(&mut self) {
        let setting = ConnectionsSetting::snapshot();
        let Some(index) = Self::selected_column_index(&setting) else {
            return;
        };
        self.pending_column_width_deltas.remove(&index);
        if !setting.column_widths.contains_key(&index) {
            return;
        }

        ConnectionsSetting::update(|setting| {
            setting.column_widths.remove(&index);
        });
        self.schedule_layout_save();
    }

    fn schedule_layout_save(&mut self) {
        self.layout_save_ticks_remaining = LAYOUT_SAVE_TICKS;
    }

    fn tick_layout_save(&mut self) {
        if self.layout_save_ticks_remaining == 0 {
            return;
        }

        self.layout_save_ticks_remaining -= 1;
        if self.layout_save_ticks_remaining == 0 {
            self.notify_connections_layout_changed();
        }
    }

    fn flush_layout_save(&mut self) {
        if self.layout_save_ticks_remaining == 0 {
            return;
        }

        self.layout_save_ticks_remaining = 0;
        self.notify_connections_layout_changed();
    }

    fn notify_connections_layout_changed(&self) {
        if let Some(tx) = &self.action_tx {
            let _ = tx.send(Action::ConnectionsLayoutChanged);
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
            ])
            .compact(vec![
                Fragment::hl(arrow::UP),
                Fragment::raw("/"),
                Fragment::hl("PU"),
                Fragment::raw("/"),
                Fragment::hl("g"),
                Fragment::raw("/"),
                Fragment::hl("G"),
                Fragment::raw("/"),
                Fragment::hl("PD"),
                Fragment::raw("/"),
                Fragment::hl(arrow::DOWN),
            ]),
            Shortcut::new(vec![
                Fragment::hl(arrow::LEFT),
                Fragment::raw("/"),
                Fragment::hl(arrow::RIGHT),
                Fragment::raw(" sort "),
                Fragment::hl("r"),
            ]),
            Shortcut::new(vec![Fragment::hl("-/+"), Fragment::raw(" width")])
                .compact(vec![Fragment::hl("-/+"), Fragment::raw(" w")]),
            Shortcut::new(vec![Fragment::hl("Del"), Fragment::raw(" reset")])
                .compact(vec![Fragment::hl("Del"), Fragment::raw(" rst")]),
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
            KeyCode::Char('-') if key.modifiers == KeyModifiers::NONE => {
                self.adjust_column_width(-1);
            }
            KeyCode::Char('=') if key.modifiers == KeyModifiers::NONE => {
                self.adjust_column_width(1);
            }
            KeyCode::Char('+')
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.adjust_column_width(1);
            }
            KeyCode::Delete if key.modifiers == KeyModifiers::NONE => self.reset_column_width(),
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
            Action::Quit => {
                self.token.cancel();
                self.flush_layout_save();
            }
            Action::Tick => {
                if self.live_mode.load(Ordering::Relaxed) {
                    self.live_throbber.calc_next();
                }
                self.tick_layout_save();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::query::QueryState;

    fn connection_col_index(id: &str) -> usize {
        CONNECTION_COLS
            .iter()
            .position(|def| def.col.id == id)
            .unwrap_or_else(|| panic!("connection column {id:?} should exist"))
    }

    fn component() -> ConnectionsComponent {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        ConnectionsComponent::new(Arc::new(AsyncMutex::new(rx)), NonZeroUsize::new(1).unwrap())
    }

    fn setting() -> ConnectionsSetting {
        let columns =
            vec![ALIVE_COLUMN_INDEX, connection_col_index("host"), connection_col_index("rule")];
        ConnectionsSetting {
            query_state: QueryState { pattern: None, sort: None, max_cols: columns.len() },
            columns,
            column_widths: HashMap::new(),
            source_ip_alias: HashMap::new(),
        }
    }

    #[test]
    fn pending_width_deltas_accumulate_and_cancel_out() {
        let mut component = component();
        let host = connection_col_index("host");

        component.queue_column_width_delta(host, 1);
        component.queue_column_width_delta(host, 1);
        component.queue_column_width_delta(host, -1);
        assert_eq!(component.pending_column_width_deltas.get(&host), Some(&1));

        component.queue_column_width_delta(host, -1);
        assert!(!component.pending_column_width_deltas.contains_key(&host));
    }

    #[test]
    fn layout_save_waits_four_ticks_and_resets_countdown() {
        let mut component = component();
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        component.action_tx = Some(action_tx);

        component.schedule_layout_save();
        for _ in 0..3 {
            component.update(Action::Tick).unwrap();
        }
        assert!(action_rx.try_recv().is_err());

        component.schedule_layout_save();
        for _ in 0..3 {
            component.update(Action::Tick).unwrap();
        }
        assert!(action_rx.try_recv().is_err());

        component.update(Action::Tick).unwrap();
        assert!(matches!(action_rx.try_recv(), Ok(Action::ConnectionsLayoutChanged)));

        component.update(Action::Tick).unwrap();
        assert!(action_rx.try_recv().is_err());
    }

    #[test]
    fn quit_flushes_pending_layout_save() {
        let mut component = component();
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        component.action_tx = Some(action_tx);
        component.schedule_layout_save();

        component.update(Action::Quit).unwrap();

        assert!(matches!(action_rx.try_recv(), Ok(Action::ConnectionsLayoutChanged)));
        assert_eq!(component.layout_save_ticks_remaining, 0);
    }

    #[test]
    fn pending_width_delta_prefers_latest_fixed_width() {
        let mut component = component();
        let mut setting = setting();
        let host = connection_col_index("host");
        component.queue_column_width_delta(host, -3);
        setting.column_widths.insert(host, 28);
        let constraints = component.table_constraints(&setting);

        assert_eq!(
            component.resolve_pending_column_widths(&setting, &constraints, Rect::default()),
            Some(vec![(host, 1, 25)])
        );
    }

    #[test]
    fn pending_width_deltas_use_current_layout_and_ignore_invisible_columns() {
        let mut component = component();
        let setting = setting();
        let constraints = component.table_constraints(&setting);
        let area = Rect::new(0, 0, 80, 1);
        let rendered = Layout::horizontal(constraints.iter().copied())
            .flex(TABLE_FLEX)
            .spacing(COLUMN_SPACING)
            .split(area);
        let host = connection_col_index("host");
        component.queue_column_width_delta(host, 2);
        component.queue_column_width_delta(connection_col_index("process"), 1);

        assert_eq!(
            component.resolve_pending_column_widths(&setting, &constraints, area),
            Some(vec![(host, 1, rendered[1].width + 2)])
        );
        assert!(component.pending_column_width_deltas.is_empty());
    }
}
