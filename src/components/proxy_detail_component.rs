use std::sync::Arc;

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::{Color, Line, Span};
use ratatui::style::{Style, Stylize};
use ratatui::symbols::line;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Scrollbar, ScrollbarOrientation};
use throbber_widgets_tui::{BLACK_CIRCLE, BRAILLE_SIX, Throbber, ThrobberState, WhichUse};

use crate::action::Action;
use crate::components::proxy_setting::get_proxy_setting;
use crate::components::shortcut::{Fragment, Shortcut};
use crate::components::{Component, ComponentId};
use crate::models::proxy::Proxy;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT, popup_area, space_between};
use crate::widgets::scrollbar::ScrollState;

const CARD_HEIGHT: u16 = 3;
const CARD_WIDTH: u16 = 25;

#[derive(Debug, Default)]
pub struct ProxyDetailComponent {
    show: bool,

    proxy: Option<Arc<Proxy>>,
    store: Option<Vec<Arc<Proxy>>>,

    focused: Option<usize>,
    scroll_state: ScrollState,

    loading: bool,
    throbber_state: ThrobberState,

    pending_test: u16,
    throbber_state_test: ThrobberState,
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
        self.focused = None;
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

    fn render_loading_throbber(&mut self, frame: &mut Frame, area: Rect) {
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
                &mut self.throbber_state_test,
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
                &mut self.throbber_state,
            );
        }
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .track_symbol(Some(line::VERTICAL))
            .begin_symbol(Some(arrow::UP))
            .end_symbol(Some(arrow::DOWN));
        frame.render_stateful_widget(scrollbar, area, &mut self.scroll_state.state);
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
        self.scroll_state
            .step(cols)
            .length(children.len(), ((area.height / CARD_HEIGHT) as usize) * cols);
        let children = &children[self.scroll_state.pos()..self.scroll_state.end_pos()];
        for (idx, child) in children.iter().enumerate() {
            let row = (idx / cols) as u16;
            let col = idx % cols;

            // Calculate card area
            let mut rect = col_chunks[col];
            rect.y += row * CARD_HEIGHT;
            rect.height = CARD_HEIGHT;
            if rect.y + rect.height > area.y + area.height {
                break; // Don't render outside the area
            }

            let focused = {
                let idx = self.scroll_state.pos() + idx;
                self.focused.is_some_and(|v| v == idx)
            };
            self.render_card(child, focused, frame, rect);
        }
    }

    fn next(&mut self, step: usize) {
        let focused = self
            .focused
            .map(|v| v.saturating_add(step).min(self.scroll_state.content_length() - 1))
            .unwrap_or_default();
        self.focused = Some(focused);
        if focused >= self.scroll_state.end_pos() {
            self.scroll_state.next();
        }
    }

    fn prev(&mut self, step: usize) {
        let focused = self.focused.map(|v| v.saturating_sub(step)).unwrap_or_default();
        self.focused = Some(focused);
        if focused < self.scroll_state.pos() {
            self.scroll_state.prev();
        }
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
            Shortcut::new(vec![Fragment::raw("first "), Fragment::hl("g")]),
            Shortcut::new(vec![Fragment::raw("last "), Fragment::hl("G")]),
            Shortcut::new(vec![Fragment::raw("select "), Fragment::hl("↵")]),
            Shortcut::from("refresh", 0).unwrap(),
            Shortcut::from("test", 0).unwrap(),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('q') => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Esc => {
                if self.focused.is_some() {
                    self.focused = None;
                } else {
                    self.hide();
                    return Ok(Some(Action::Unfocus));
                }
            }
            KeyCode::Char('g') => {
                if self.focused.is_some() {
                    self.focused = Some(0)
                }
                self.scroll_state.first();
            }
            KeyCode::Char('G') => {
                if self.focused.is_some() {
                    self.focused = Some(self.scroll_state.content_length() - 1)
                }
                self.scroll_state.last();
            }
            KeyCode::Char('j') | KeyCode::Down => self.next(self.scroll_state.step_value()),
            KeyCode::Char('k') | KeyCode::Up => self.prev(self.scroll_state.step_value()),
            KeyCode::Char('h') | KeyCode::Left => self.prev(1),
            KeyCode::Char('l') | KeyCode::Right => self.next(1),
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
                    let action = self.focused.and_then(|idx| {
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
                let action = match (self.focused, self.proxy.as_ref(), self.store.as_ref()) {
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
                    self.throbber_state.calc_next();
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
        self.render_loading_throbber(frame, area);

        self.render_cards(frame, content_area);
        self.render_scrollbar(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
