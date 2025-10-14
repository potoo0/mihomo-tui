use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::{Color, Line, Span};
use ratatui::style::{Style, Stylize};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use throbber_widgets_tui::{BLACK_CIRCLE, BRAILLE_SIX, Throbber, ThrobberState, WhichUse};

use crate::action::Action;
use crate::components::proxy_setting::get_proxy_setting;
use crate::components::{Component, ComponentId};
use crate::models::proxy::Proxy;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT, popup_area, space_between};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const CARD_HEIGHT: u16 = 3;
const CARD_WIDTH: u16 = 25;

#[derive(Debug, Default)]
pub struct ProxyDetailComponent {
    show: bool,

    proxy: Option<Arc<Proxy>>,
    store: Option<Vec<Arc<Proxy>>>,
    navigator: ScrollableNavigator,

    loading: bool,
    throbber: ThrobberState,

    pending_test: u16,
    pending_test_throbber: ThrobberState,
}

impl ProxyDetailComponent {
    pub fn show(&mut self, proxy: Arc<Proxy>, store: Vec<Arc<Proxy>>) {
        tracing::debug!("Show proxy detail: {}, loading: {}", proxy.name, self.loading);
        self.show = true;
        self.proxy = Some(proxy);
        self.store = Some(store);

        self.loading = false;
        self.pending_test = self.pending_test.saturating_sub(1);
    }

    pub fn hide(&mut self) {
        self.show = false;
        self.proxy = None;
        self.store = None;

        self.navigator.focused = None;
        self.navigator.scroller.position(0);
    }

    fn title_line(&'_ self) -> Line<'_> {
        let proxy = self.proxy.as_ref().unwrap();
        Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::styled(proxy.name.as_str(), Color::White),
            Span::raw(" ("),
            Span::styled(
                format!("{}", proxy.children.as_ref().map_or(0, Vec::len)),
                Color::LightCyan,
            ),
            Span::raw(")"),
            Span::raw(TOP_TITLE_RIGHT),
        ])
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if self.pending_test > 0 {
            let symbol = Throbber::default()
                .label("Testing")
                .style(Style::default().fg(Color::White).bg(Color::Green).bold())
                .throbber_style(Style::default().fg(Color::White).bg(Color::Green).bold())
                .throbber_set(BLACK_CIRCLE)
                .use_type(WhichUse::Spin);
            frame.render_stateful_widget(
                symbol,
                Rect::new(area.right().saturating_sub(20), area.y, 9, 1),
                &mut self.pending_test_throbber,
            );
        }
        if self.loading {
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
    }

    fn is_selected(&self, name: &str) -> bool {
        self.proxy.as_ref().and_then(|v| v.selected.as_deref()).is_some_and(|v| v == name)
    }

    fn render_card(&self, proxy: &Proxy, focused: bool, frame: &mut Frame, area: Rect) {
        let selected = self.is_selected(&proxy.name);
        let (border_type, border_color) = if focused {
            (BorderType::Thick, Color::Cyan)
        } else if selected {
            (BorderType::Rounded, Color::Green)
        } else {
            (BorderType::Rounded, Color::DarkGray)
        };
        let title_style = if selected { Color::Green } else { Color::default() };
        let block = Block::bordered()
            .border_type(border_type)
            .border_style(border_color)
            .title_top(Span::styled(proxy.name.as_str(), title_style));

        let threshold = get_proxy_setting().read().unwrap().threshold;
        let para = Paragraph::new(space_between(
            area.width - 2, // minus border
            Span::raw(proxy.r#type.as_str()),
            proxy.latency.as_span(threshold),
        ))
        .block(block);
        frame.render_widget(para, area);
    }

    fn render_cards(&mut self, frame: &mut Frame, area: Rect) {
        let children = match self.store.as_ref() {
            None => return,
            Some(v) => v,
        };

        let cols = (area.width / CARD_WIDTH).max(1) as usize;
        let col_chunks =
            Layout::horizontal((0..cols).map(|_| Constraint::Min(CARD_WIDTH))).split(area);
        self.navigator
            .step(cols)
            .length(children.len(), ((area.height / CARD_HEIGHT) as usize) * cols);
        self.navigator.iter_visible(children, CARD_HEIGHT, col_chunks).for_each(
            |(proxy, focused, rect)| {
                self.render_card(proxy, focused, frame, rect);
            },
        );
    }
}

impl Component for ProxyDetailComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ProxyDetail
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
            Shortcut::new(vec![Fragment::hl("g"), Fragment::raw(" jump "), Fragment::hl("G")]),
            Shortcut::new(vec![
                Fragment::hl("PgUp"),
                Fragment::raw(" page "),
                Fragment::hl("PgDn"),
            ]),
            Shortcut::new(vec![Fragment::raw("select "), Fragment::hl("↵")]),
            Shortcut::new(vec![Fragment::raw("back "), Fragment::hl("Esc")]),
            Shortcut::from("refresh", 0).unwrap(),
            Shortcut::from("test", 0).unwrap(),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.navigator.handle_key_event(true, key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('q') => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Esc => {
                if self.navigator.focused.is_some() {
                    self.navigator.focused = None;
                } else {
                    self.hide();
                    return Ok(Some(Action::Unfocus));
                }
            }
            KeyCode::Char('r') => {
                if !self.loading {
                    self.loading = true;
                    return Ok(Some(Action::ProxiesRefresh));
                }
            }
            KeyCode::Enter => {
                if !self.loading {
                    // switch to selected proxy
                    let selector_name = self.proxy.as_ref().unwrap().name.clone();
                    let action = self.navigator.focused.and_then(|idx| {
                        self.store
                            .as_ref()
                            .and_then(|v| v.get(idx))
                            .map(|v| Action::ProxyUpdateRequest(selector_name, v.name.clone()))
                    });
                    self.loading = action.is_some();
                    return Ok(action);
                }
            }
            KeyCode::Char('t') => {
                let action =
                    match (self.navigator.focused, self.proxy.as_ref(), self.store.as_ref()) {
                        (Some(focused), _, Some(store)) => {
                            store.get(focused).map(|p| Action::ProxyTestRequest(p.name.clone()))
                        }
                        (None, Some(proxy), _) => {
                            Some(Action::ProxyGroupTestRequest(proxy.name.clone()))
                        }
                        _ => None,
                    };
                self.pending_test = self.pending_test.saturating_add(1);
                return Ok(action);
            }
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::ProxyDetail(p, store) => self.show(p, store),
            Action::Tick => {
                if self.loading {
                    self.throbber.calc_next();
                }
            }
            _ => (),
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.show || self.proxy.is_none() {
            return Ok(());
        }

        let area = popup_area(area, 80, 80);
        frame.render_widget(Clear, area); // clears out the background
        // outer margin
        let area = area.inner(Margin::new(2, 1));

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(self.title_line());
        let content_area = block.inner(area);
        frame.render_widget(block, area);
        self.render_throbber(frame, area);

        self.render_cards(frame, content_area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
