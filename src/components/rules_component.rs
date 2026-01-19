use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexMap;
use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::style::Stylize;
use ratatui::widgets::{Block, BorderType, Cell, Row, Table, TableState};
use throbber_widgets_tui::{BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::rules::{RULE_COLS, Rules};
use crate::components::{Component, ComponentId};
use crate::models::Rule;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

#[derive(Default)]
pub struct RulesComponent {
    api: Option<Arc<Api>>,
    store: Arc<Rules>,
    filter_pattern_changed: bool,
    filter_pattern: Arc<Mutex<Option<String>>>,

    navigator: ScrollableNavigator,
    table_state: TableState,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,

    action_tx: Option<UnboundedSender<Action>>,
}

impl RulesComponent {
    fn load_rules(&mut self) -> Result<()> {
        info!("Loading rules");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let filter_pattern = Arc::clone(&self.filter_pattern);
        let loading = Arc::clone(&self.loading);
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("rule-loader").spawn(async move {
            Self::refresh_rules(&api, &store, &filter_pattern).await;
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    async fn refresh_rules(
        api: &Api,
        store: &Arc<Rules>,
        filter_pattern: &Arc<Mutex<Option<String>>>,
    ) {
        match api.get_rules().await {
            Ok(rules) => {
                store.push(rules);
                // initial view
                let filter_pattern = filter_pattern.lock().unwrap();
                let filter_pattern = filter_pattern.as_deref();
                store.compute_view(filter_pattern);
            }
            Err(e) => warn!(error = ?e, "Failed to get rules"),
        }
    }

    fn toggle_disabled(&mut self) {
        fn toggle(rule: &Rule) {
            rule.disable_state
                .store(!rule.disable_state.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        if !self.store.supports_disable() {
            warn!(
                meta_version_required = ">= v1.19.19",
                upstream_pr = 2502,
                "Rule disabling is not supported by the current rule model"
            );
            return;
        }
        if let Some(idx) = self.navigator.focused {
            debug!("Toggling rule disabled state at index {}", idx);
            self.store.with_view(|records| toggle(&records[idx]))
        } else {
            debug!("Toggling all rule disabled state.");
            self.store.with_view(|records| {
                for record in records {
                    toggle(record);
                }
            });
        }
    }

    fn collect_disabled_changes(&self) -> IndexMap<usize, bool> {
        let mut state: IndexMap<usize, bool> = IndexMap::new();
        self.store.with_view(|records| {
            for r in records {
                if let (Some(index), Some(extra)) = (r.index, r.extra.as_ref()) {
                    let ui = r.disable_state.load(Ordering::Relaxed);
                    if ui != extra.disabled {
                        state.insert(index, ui);
                    }
                }
            }
        });
        state
    }

    fn submit_disabled_changes(&mut self) -> Result<()> {
        if self.loading.load(Ordering::Relaxed) {
            warn!("Rule operations are in progress, disabled change submission is skipped");
            return Ok(());
        }

        if !self.store.supports_disable() {
            warn!(
                meta_version_required = ">= v1.19.19",
                upstream_pr = 2502,
                "Rule disabling is not supported by the current rule model"
            );
            return Ok(());
        }
        let changes = self.collect_disabled_changes();
        info!("Submitting disabled rule changes: {:?}", changes);
        if changes.is_empty() {
            return Ok(());
        }

        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let filter_pattern = Arc::clone(&self.filter_pattern);
        let loading = Arc::clone(&self.loading);
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("rule-disabled-change-submitter").spawn(async move {
            match api.update_rules_disabled_state(changes).await {
                Ok(_) => {
                    info!("Successfully applied disabled rule changes");
                    Self::refresh_rules(&api, &store, &filter_pattern).await;
                }
                Err(e) => warn!(error = ?e, "Failed to apply disabled rule changes"),
            }
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if !self.loading.load(Ordering::Relaxed) {
            return;
        }

        let symbol = Throbber::default()
            .label("Loading")
            .style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .throbber_style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .throbber_set(BRAILLE_SIX)
            .use_type(WhichUse::Spin);
        frame.render_stateful_widget(
            symbol,
            Rect::new(area.right().saturating_sub(10), area.y, 9, 1),
            &mut self.throbber,
        );
    }

    fn render_rules(&mut self, frame: &mut Frame, area: Rect) {
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
            Span::raw("rules ("),
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
        let header = RULE_COLS
            .iter()
            .map(|def| def.title)
            .map(|title| Cell::from(title).bold())
            .collect::<Row>()
            .height(1)
            .bottom_margin(1);
        let selected_row_style = Style::default().add_modifier(Modifier::REVERSED).fg(Color::Cyan);

        let rows: Vec<Row> = records
            .iter()
            .map(|item| Row::new(RULE_COLS.iter().map(|def| (def.accessor)(item))).height(1u16))
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Length(8),
                Constraint::Min(1),
                Constraint::Percentage(8),
                Constraint::Percentage(8),
                Constraint::Percentage(8),
                Constraint::Percentage(20),
            ],
        )
        .block(block)
        .header(header)
        .column_spacing(2)
        .row_highlight_style(selected_row_style);

        frame.render_stateful_widget(table, area, &mut self.table_state);
    }
}

impl Drop for RulesComponent {
    fn drop(&mut self) {
        info!("`RulesComponent` dropped, background task cancelled");
    }
}

impl Component for RulesComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Rules
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
            Shortcut::from("toggle", 0).unwrap(),
            Shortcut::from("submit", 0).unwrap(),
        ]
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        self.load_rules()?;

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
            KeyCode::Char('r') => self.load_rules()?,
            KeyCode::Char('t') => self.toggle_disabled(),
            KeyCode::Char('s') => self.submit_disabled_changes()?,
            _ => (),
        };

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {
                if self.filter_pattern_changed {
                    debug!("handle Action::Tick, recompute rules view");
                    let filter_pattern = self.filter_pattern.lock().unwrap();
                    let filter_pattern = filter_pattern.as_deref();
                    self.store.compute_view(filter_pattern);
                    self.filter_pattern_changed = false;
                }
                if self.loading.load(Ordering::Relaxed) {
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
        self.render_rules(frame, area);
        self.render_throbber(frame, area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
