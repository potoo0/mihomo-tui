use std::sync::{Arc, Mutex};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style, Stylize};
use ratatui::widgets::{Block, BorderType, Cell, Row, Table, TableState};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::rules::{RULE_COLS, Rules};
use crate::components::{Component, ComponentId};
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

    action_tx: Option<UnboundedSender<Action>>,
}

impl RulesComponent {
    fn load_rules(&mut self) -> Result<()> {
        info!("Loading rules");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let filter_pattern = Arc::clone(&self.filter_pattern);

        tokio::task::Builder::new().name("log-loader").spawn(async move {
            match api.get_rules().await {
                Ok(rules) => {
                    store.push(rules);
                    // initial view
                    let filter_pattern = filter_pattern.lock().unwrap();
                    let filter_pattern = filter_pattern.as_deref();
                    store.compute_view(filter_pattern);
                }
                Err(e) => warn!("Failed to get rules: {e}"),
            }
        })?;

        Ok(())
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        let records = self.store.with_view(|records| {
            let len = records.len();
            self.navigator.length(len, (area.height - 4) as usize); // viewport height
            // NOTE: end_pos() depends on length()
            records
                .get(self.navigator.scroller.pos()..self.navigator.scroller.end_pos())
                .unwrap_or(&[])
                .to_vec()
        });

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
        let header = std::iter::once(Cell::from(""))
            .chain(RULE_COLS.iter().map(|def| Cell::from(def.title).bold()))
            .collect::<Row>()
            .height(1)
            .bottom_margin(1);
        let selected_style = Style::default().add_modifier(Modifier::REVERSED).fg(Color::Cyan);

        let constraints = [
            Constraint::Length(1),
            Constraint::Percentage(25),
            Constraint::Percentage(45),
            Constraint::Percentage(30),
        ];

        let rows: Vec<Row> = records
            .iter()
            .map(|rule| {
                Row::new(
                    std::iter::once(Cell::from(""))
                        .chain(RULE_COLS.iter().map(|def| Cell::from((def.accessor)(rule)))),
                )
                .height(1)
            })
            .collect();

        let table = Table::new(rows, constraints)
            .block(block)
            .header(header)
            .row_highlight_style(selected_style)
            .column_spacing(2);

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
                Fragment::raw(" nav "),
                Fragment::hl(arrow::DOWN),
            ]),
            Shortcut::new(vec![
                Fragment::hl("PgUp"),
                Fragment::raw(" page "),
                Fragment::hl("PgDn"),
            ]),
            Shortcut::new(vec![Fragment::hl("g"), Fragment::raw(" jump "), Fragment::hl("G")]),
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
            KeyCode::Char('f') => return Ok(Some(Action::Focus(ComponentId::Search))),
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
        self.render_table(frame, area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
