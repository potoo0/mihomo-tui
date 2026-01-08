use std::sync::{Arc, Mutex};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::rules::Rules;
use crate::components::{Component, ComponentId, HORIZ_STEP};
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

    horiz_offset: usize,
    navigator: ScrollableNavigator,
    list_state: ListState,

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
                    store.push(rules.rules);
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

    fn render_list(&mut self, frame: &mut Frame, area: Rect) {
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

        let items: Vec<ListItem> = records
            .iter()
            .map(|rule| {
                let mut content = String::with_capacity(
                    rule.r#type.len() + rule.payload.len() + rule.proxy.len() + 2,
                );
                content.push_str(&rule.r#type);
                if !rule.payload.is_empty() {
                    content.push(',');
                    content.push_str(&rule.payload);
                }
                content.push(',');
                content.push_str(&rule.proxy);
                let content = if content.len() > self.horiz_offset {
                    &content[self.horiz_offset..]
                } else {
                    ""
                };
                ListItem::new(content.to_string())
            })
            .collect();
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
        let selected_style = Style::default().add_modifier(Modifier::REVERSED).fg(Color::Cyan);
        let logs = List::new(items).block(block).highlight_style(selected_style);
        *self.list_state.selected_mut() =
            self.navigator.focused.map(|v| v.saturating_sub(self.navigator.scroller.pos()));
        frame.render_stateful_widget(logs, area, &mut self.list_state);
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
            KeyCode::Left => self.horiz_offset = self.horiz_offset.saturating_sub(HORIZ_STEP),
            KeyCode::Right => self.horiz_offset = self.horiz_offset.saturating_add(HORIZ_STEP),
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
        self.render_list(frame, area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
