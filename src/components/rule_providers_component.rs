use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style, Stylize};
use ratatui::widgets::{Block, BorderType, Cell, Row, Table, TableState};
use throbber_widgets_tui::{BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::rule_providers::{RULE_PROVIDER_COLS, RuleProviders};
use crate::components::{Component, ComponentId};
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

#[derive(Default)]
pub struct RuleProvidersComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,

    store: Arc<RuleProviders>,
    filter_pattern_changed: bool,
    filter_pattern: Arc<Mutex<Option<String>>>,

    navigator: ScrollableNavigator,
    table_state: TableState,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,
    pending_update: Arc<RwLock<HashMap<String, usize>>>,
}

impl RuleProvidersComponent {
    fn load_rule_providers(&mut self) -> Result<()> {
        info!("Loading rule providers");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let filter_pattern = Arc::clone(&self.filter_pattern);
        let loading = Arc::clone(&self.loading);
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("log-loader").spawn(async move {
            Self::refresh_rule_providers(&api, &store, &filter_pattern).await;
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn update_rule_providers(&mut self) {
        let names = self.collect_update_names();
        if names.is_empty() {
            return;
        }
        debug!("updating rule providers: {:?}", names);

        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let filter_pattern = Arc::clone(&self.filter_pattern);
        let pending_update = Arc::clone(&self.pending_update);
        // update counter
        {
            let mut guard = pending_update.write().unwrap();
            names.iter().for_each(|name| *guard.entry(name.clone()).or_insert(0) += 1);
        }

        tokio::spawn(async move {
            // update
            for name in names.iter() {
                if let Err(e) = api.update_rule_provider(name).await {
                    error!(error = ?e, provider = name, "update rule provider failed");
                }
                {
                    let mut guard = pending_update.write().unwrap();
                    if let Some(entry) = guard.get_mut(name) {
                        *entry = entry.saturating_sub(1);
                        if *entry == 0 {
                            guard.remove(name);
                        }
                    }
                }
            }

            // refresh providers
            Self::refresh_rule_providers(&api, &store, &filter_pattern).await;
        });
    }

    async fn refresh_rule_providers(
        api: &Api,
        store: &RuleProviders,
        filter_pattern: &Mutex<Option<String>>,
    ) {
        match api.get_rule_providers().await {
            Ok(providers) => {
                store.push(providers);
                let filter_pattern = filter_pattern.lock().unwrap();
                store.compute_view(filter_pattern.as_deref());
            }
            Err(e) => error!(error = ?e, "Failed to get rule providers"),
        }
    }

    fn collect_update_names(&self) -> Vec<String> {
        if let Some(idx) = self.navigator.focused {
            debug!("updating rule provider at index {}", idx);
            vec![self.store.with_view(|records| records[idx].name.clone())]
        } else {
            debug!("updating all visible rule providers");
            self.store.with_view(|records| {
                records
                    .get(self.navigator.scroller.pos()..self.navigator.scroller.end_pos())
                    .into_iter()
                    .flatten()
                    .map(|r| r.name.clone())
                    .collect()
            })
        }
    }

    fn is_busy(&self) -> bool {
        self.loading.load(Ordering::Relaxed)
            || !self.pending_update.read().map(|m| m.is_empty()).unwrap_or(true)
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if !self.is_busy() {
            return;
        }
        let label = if self.loading.load(Ordering::Relaxed) { "Loading" } else { "Updating" };
        let symbol = Throbber::default()
            .label(label)
            .style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .throbber_style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .throbber_set(BRAILLE_SIX)
            .use_type(WhichUse::Spin);
        frame.render_stateful_widget(
            symbol,
            Rect::new(area.right().saturating_sub(12), area.y, 11, 1),
            &mut self.throbber,
        );
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        let records = self.store.with_view(|records| {
            let len = records.len();
            // update scroller, viewport = area.height - 2 (border)
            self.navigator.length(len, (area.height - 2) as usize);
            // NOTE: end_pos() depends on length()
            records
                .get(self.navigator.scroller.pos()..self.navigator.scroller.end_pos())
                .unwrap_or(&[])
                .to_vec()
        });

        // update table selected, which is relative position in current viewport
        *self.table_state.selected_mut() =
            self.navigator.focused.map(|v| v.saturating_sub(self.navigator.scroller.pos()));

        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw("rule providers ("),
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
        let header = RULE_PROVIDER_COLS
            .iter()
            .map(|def| def.title)
            .map(|title| Cell::from(title).bold())
            .collect::<Row>()
            .height(1)
            .bottom_margin(1);
        let selected_row_style = Style::default().add_modifier(Modifier::REVERSED).fg(Color::Cyan);

        let rows: Vec<Row> = records
            .iter()
            .map(|item| {
                Row::new(RULE_PROVIDER_COLS.iter().map(|def| (def.accessor)(item))).height(1u16)
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Min(30),
                Constraint::Min(15),
                Constraint::Min(15),
                Constraint::Min(15),
                Constraint::Min(30),
            ],
        )
        .block(block)
        .header(header)
        .column_spacing(2)
        .row_highlight_style(selected_row_style);

        frame.render_stateful_widget(table, area, &mut self.table_state);
    }
}

impl Drop for RuleProvidersComponent {
    fn drop(&mut self) {
        info!("`RuleProvidersComponent` dropped, background task cancelled");
    }
}

impl Component for RuleProvidersComponent {
    fn id(&self) -> ComponentId {
        ComponentId::RuleProviders
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::hl(arrow::UP),
                Fragment::raw("/"),
                Fragment::hl(arrow::LEFT),
                Fragment::raw(" nav "),
                Fragment::hl(arrow::RIGHT),
                Fragment::raw("/"),
                Fragment::hl(arrow::DOWN),
            ]),
            Shortcut::new(vec![
                Fragment::hl("PgUp"),
                Fragment::raw(" page "),
                Fragment::hl("PgDn"),
            ]),
            Shortcut::new(vec![Fragment::hl("g"), Fragment::raw(" jump "), Fragment::hl("G")]),
            Shortcut::from("refresh", 0).unwrap(),
            Shortcut::from("update", 0).unwrap(),
        ]
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.navigator.handle_key_event(false, key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc => self.navigator.focused = None,
            KeyCode::Char('f') => return Ok(Some(Action::Focus(ComponentId::Search))),
            KeyCode::Char('r') => self.load_rule_providers()?,
            KeyCode::Char('u') => self.update_rule_providers(),
            _ => (),
        };

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {
                if self.filter_pattern_changed {
                    debug!("handle Action::Tick, recompute rule providers view");
                    let filter_pattern = self.filter_pattern.lock().unwrap();
                    let filter_pattern = filter_pattern.as_deref();
                    self.store.compute_view(filter_pattern);
                    self.filter_pattern_changed = false;
                }
                if self.is_busy() {
                    self.throbber.calc_next();
                }
            }
            Action::SearchInputChanged(pattern) => {
                debug!("handle Action::SearchInputChanged, got pattern={pattern:?}");
                *self.filter_pattern.lock().unwrap() = pattern;
                self.filter_pattern_changed = true;
            }
            Action::TabSwitch(to) => {
                if to == self.id() {
                    // reload data when switch to this component
                    self.load_rule_providers()?;
                    // send search pattern to search component
                    let pattern = self.filter_pattern.lock().unwrap().clone();
                    debug!("handle Action::TabSwitch, current search pattern={pattern:?}");
                    return Ok(Some(Action::SearchInputSet(pattern)));
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
