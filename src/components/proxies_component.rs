use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::Style;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use throbber_widgets_tui::{BLACK_CIRCLE, BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId};
use crate::store::proxies::{Proxies, ProxyView};
use crate::store::proxy_setting::ProxySetting;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const CARD_HEIGHT: u16 = 4;
const CARDS_PER_ROW: usize = 2;

#[derive(Debug)]
pub struct ProxiesComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,
    navigator: ScrollableNavigator,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,

    pending_test: Arc<AtomicU16>,
    pending_test_throbber: ThrobberState,
}

impl Default for ProxiesComponent {
    fn default() -> Self {
        Self {
            api: None,
            action_tx: None,
            navigator: ScrollableNavigator::new(CARDS_PER_ROW),
            loading: Default::default(),
            throbber: Default::default(),
            pending_test: Default::default(),
            pending_test_throbber: Default::default(),
        }
    }
}

impl ProxiesComponent {
    fn load_proxies(&mut self) -> Result<()> {
        self.loading.store(true, Ordering::Relaxed);
        info!("Loading proxies");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let loading = Arc::clone(&self.loading);

        tokio::task::Builder::new().name("proxies-loader").spawn(async move {
            if let Err(e) = Proxies::load(api).await {
                error!(error = ?e, "Failed to load proxies");
            }
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn test_proxy_group(&self, name: String) -> Result<()> {
        info!("Testing proxy group {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let pending_test = Arc::clone(&self.pending_test);
        pending_test.fetch_add(1, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-group-tester").spawn(async move {
            if let Err(e) = Proxies::test_group_and_reload(api, &name).await {
                error!(error = ?e, "Failed to test and load proxy: {}", name);
            }
            let _ = pending_test.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |x| {
                if x == 0 { None } else { Some(x - 1) }
            });
        })?;

        Ok(())
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if self.pending_test.load(Ordering::Relaxed) > 0 {
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
        if self.loading.load(Ordering::Relaxed) {
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

    fn render_proxy(
        threshold: (u64, u64),
        view: &ProxyView,
        focused: bool,
        frame: &mut Frame,
        area: Rect,
    ) {
        let title_line = Line::from(vec![
            Span::styled(view.proxy.name.as_str(), Color::White),
            Span::raw(" ("),
            Span::styled(
                format!("{}", view.proxy.children.as_ref().map_or(0, Vec::len)),
                Color::LightCyan,
            ),
            Span::raw(")"),
        ]);
        let (border_type, border_color) = if focused {
            (BorderType::Thick, Color::Cyan)
        } else {
            (BorderType::Rounded, Color::DarkGray)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(border_color)
            .title(title_line);
        let mut lines = vec![Line::from(vec![Span::raw(&view.proxy.r#type)])];
        if let Some(selected) = view.proxy.selected.as_ref() {
            lines[0].push_span(Span::styled(" > ", Color::DarkGray));
            lines[0].push_span(Span::styled(selected.as_str(), Color::Cyan));
        }

        let children = view.proxy.children.as_ref().map(|v| v.len()).unwrap_or(0);
        if children > 0 {
            let latency_span: Span = view.proxy.latency.as_span(threshold);
            let width = area.width - 10;
            let padding_width = (10usize - 2).saturating_sub(latency_span.width());
            let mut stats: Line = view.quality_stats.as_line(width, children);
            stats.push_span(Span::raw(" ".repeat(padding_width)));
            stats.push_span(latency_span);
            lines.push(stats);
        }
        let para = Paragraph::new(lines).block(block);
        frame.render_widget(para, area);
    }

    fn render_proxies(&mut self, frame: &mut Frame, outer: Rect) {
        let proxies_len = Proxies::with_view(|p| p.len());
        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw("proxies ("),
            Span::styled(format!("{}", proxies_len), Color::LightCyan),
            Span::raw(")"),
            Span::raw(TOP_TITLE_RIGHT),
        ]);
        let block = Block::bordered().border_type(BorderType::Rounded).title(title_line);
        let area = block.inner(outer);
        frame.render_widget(block, outer);

        let col_chunks =
            Layout::horizontal((0..CARDS_PER_ROW).map(|_| Constraint::Fill(1))).split(area);
        self.navigator
            .length(proxies_len, ((area.height / CARD_HEIGHT) as usize) * col_chunks.len());
        let proxies = Proxies::with_view(|records| {
            records
                .get(self.navigator.scroller.pos()..self.navigator.scroller.end_pos())
                .map(|slice| slice.to_vec())
                .unwrap_or_default()
        });
        let threshold = ProxySetting::global().read().unwrap().threshold;
        self.navigator.iter_layout(&proxies, CARD_HEIGHT, col_chunks).for_each(
            |(proxy, focused, rect)| {
                Self::render_proxy(threshold, proxy, focused, frame, rect);
            },
        );
    }
}

impl Drop for ProxiesComponent {
    fn drop(&mut self) {
        info!("`ProxiesComponent` dropped");
        match Proxies::global().write() {
            Ok(mut p) => p.clear(),
            Err(_) => warn!("Failed to acquire write lock to clear proxies store"),
        }
    }
}

impl Component for ProxiesComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Proxies
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
            Shortcut::new(vec![Fragment::raw("detail "), Fragment::hl("↵")]),
            Shortcut::from("refresh", 0).unwrap(),
            Shortcut::from("setting", 0).unwrap(),
            Shortcut::from("test", 0).unwrap(),
        ]
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        self.load_proxies()?;
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.navigator.handle_key_event(true, key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc => self.navigator.focused = None,
            KeyCode::Char('r') => self.load_proxies()?,
            KeyCode::Char('s') => return Ok(Some(Action::ProxySetting)),
            KeyCode::Enter => {
                let action = self
                    .navigator
                    .focused
                    .and_then(Proxies::get)
                    .map(|v| Action::ProxyDetail(v.proxy.name.clone()));
                return Ok(action);
            }
            KeyCode::Char('t') => {
                if let Some(name) =
                    self.navigator.focused.and_then(Proxies::get).map(|v| v.proxy.name.clone())
                {
                    self.test_proxy_group(name)?;
                }
            }
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::ProxySettingChanged => self.load_proxies()?,
            Action::Tick => {
                if self.loading.load(Ordering::Relaxed) {
                    self.throbber.calc_next();
                }
                if self.pending_test.load(Ordering::Relaxed) > 0 {
                    self.pending_test_throbber.calc_next();
                }
            }
            _ => (),
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render_proxies(frame, area);
        self.render_throbber(frame, area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
