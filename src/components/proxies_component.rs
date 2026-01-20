use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::Style;
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use throbber_widgets_tui::{BLACK_CIRCLE, BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::proxies::{Proxies, ProxyView};
use crate::components::proxy_setting::get_proxy_setting;
use crate::components::{Component, ComponentId};
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const CARD_HEIGHT: u16 = 4;
const CARDS_PER_ROW: usize = 2;
const NOP: fn() = || {};

#[derive(Debug)]
pub struct ProxiesComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,
    store: Arc<RwLock<Proxies>>,
    navigator: ScrollableNavigator,
    detail_focused: Option<usize>,

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
            store: Default::default(),
            navigator: ScrollableNavigator::new(CARDS_PER_ROW),
            detail_focused: None,
            loading: Default::default(),
            throbber: Default::default(),
            pending_test: Default::default(),
            pending_test_throbber: Default::default(),
        }
    }
}

impl ProxiesComponent {
    fn load_proxies(&mut self) -> Result<()> {
        info!("Loading proxies");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);

        tokio::task::Builder::new()
            .name("proxies-loader")
            .spawn(Self::load_proxies_task(api, store, NOP))?;

        Ok(())
    }

    async fn load_proxies_task<F>(api: Arc<Api>, store: Arc<RwLock<Proxies>>, cb: F) -> Result<()>
    where
        F: FnOnce(),
    {
        // match tokio::try_join!(api.get_proxies(), api.get_providers()) {
        match api.get_proxies().await {
            Ok(proxies) => {
                store.write().unwrap().push(proxies);
                cb();
                Ok(())
            }
            Err(e) => {
                error!(error = ?e, "Failed to get proxies");
                Err(e)
            }
        }
    }

    fn refresh_proxies(&self) -> Result<()> {
        info!("Refresh proxies");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        let focused = self.detail_focused;
        let loading = Arc::clone(&self.loading);

        tokio::task::Builder::new().name("proxies-refresher").spawn(Self::load_proxies_task(
            api,
            store,
            move || {
                if let Some(focused) = focused {
                    let _ = action_tx.send(Action::ProxyDetailRefresh(focused));
                }
                loading.store(false, Ordering::Relaxed);
            },
        ))?;

        Ok(())
    }

    fn update_proxies(&mut self, selector_name: String, name: String) -> Result<()> {
        info!("Updating proxies");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        let focused = self.detail_focused;

        tokio::task::Builder::new().name("proxy-updater").spawn(async move {
            match api.update_proxy(selector_name, name).await {
                Ok(_) => {
                    let _ = Self::load_proxies_task(api, store, || {
                        if let Some(focused) = focused {
                            let _ = action_tx.send(Action::ProxyDetailRefresh(focused));
                        }
                    })
                    .await;
                }
                Err(e) => {
                    error!(error = ?e, "Failed to update proxy");
                    let _ = action_tx.send(Action::Error(("Select proxy", e).into()));
                }
            }
        })?;
        Ok(())
    }

    fn test_proxy(&self, name: String, is_group: bool) -> Result<()> {
        info!("Testing proxy {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        let focused = self.detail_focused;
        let (test_url, test_timeout) = {
            let setting = get_proxy_setting().read().unwrap();
            (setting.test_url.clone(), setting.test_timeout)
        };
        let pending_test = Arc::clone(&self.pending_test);
        if focused.is_none() {
            pending_test.fetch_add(1, Ordering::Relaxed);
        }

        tokio::task::Builder::new().name("proxy-tester").spawn(async move {
            let result = if is_group {
                api.test_proxy_group(name, test_url, test_timeout).await.map(|_| ())
            } else {
                api.test_proxy(name, test_url, test_timeout).await.map(|_| ())
            };
            match result {
                Ok(_) => {
                    let _ = Self::load_proxies_task(api, store, || {
                        let _ =
                            pending_test.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |x| {
                                if x == 0 { None } else { Some(x - 1) }
                            });
                        if let Some(focused) = focused {
                            let _ = action_tx.send(Action::ProxyDetailRefresh(focused));
                        }
                    })
                    .await;
                }
                Err(e) => error!(error = ?e, "Failed to test proxy"),
            }
        })?;
        Ok(())
    }

    fn proxy_detail_action(&mut self) -> Option<Action> {
        self.detail_focused = self.navigator.focused;
        let store = self.store.read().unwrap();
        self.navigator
            .focused
            .and_then(|idx| store.get(idx))
            .map(|v| Action::ProxyDetail(Arc::clone(&v.proxy), store.children(v.proxy.as_ref())))
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

    fn render_proxy(view: &ProxyView, focused: bool, frame: &mut Frame, area: Rect) {
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
            let threshold = get_proxy_setting().read().unwrap().threshold;
            let latency_span: Span = view.proxy.latency.as_span(threshold);
            let width = area.width - 10;
            let mut stats: Line = view.quality_stats.as_line(width, children);
            stats.push_span(Span::raw(" ".repeat(10 - 2 - latency_span.width())));
            stats.push_span(latency_span);
            lines.push(stats);
        }
        let para = Paragraph::new(lines).block(block);
        frame.render_widget(para, area);
    }

    fn render_proxies(&mut self, frame: &mut Frame, outer: Rect) {
        let proxies = self.store.read().unwrap().view();

        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw("proxies ("),
            Span::styled(format!("{}", proxies.len()), Color::LightCyan),
            Span::raw(")"),
            Span::raw(TOP_TITLE_RIGHT),
        ]);
        let block = Block::bordered().border_type(BorderType::Rounded).title(title_line);
        let area = block.inner(outer);
        frame.render_widget(block, outer);

        let col_chunks =
            Layout::horizontal((0..CARDS_PER_ROW).map(|_| Constraint::Fill(1))).split(area);
        self.navigator
            .length(proxies.len(), ((area.height / CARD_HEIGHT) as usize) * col_chunks.len());
        self.navigator.iter_visible(&proxies, CARD_HEIGHT, col_chunks).for_each(
            |(proxy, focused, rect)| {
                Self::render_proxy(proxy, focused, frame, rect);
            },
        );
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
        self.detail_focused = None;
        if self.navigator.handle_key_event(true, key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc => {
                self.navigator.focused = None;
                self.detail_focused = None;
            }
            KeyCode::Char('r') => {
                if self
                    .loading
                    .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    return Ok(Some(Action::ProxiesRefresh));
                }
            }
            KeyCode::Char('s') => return Ok(Some(Action::ProxySetting)),
            KeyCode::Enter => return Ok(self.proxy_detail_action()),
            KeyCode::Char('t') => {
                let store = self.store.read().unwrap();
                if let Some(idx) = self.navigator.focused {
                    let action = store
                        .get(idx)
                        .map(|v| v.proxy.name.clone())
                        .map(Action::ProxyGroupTestRequest);
                    return Ok(action);
                }
            }
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Unfocus => self.detail_focused = None,
            Action::ProxyUpdateRequest(selector_name, name) => {
                self.update_proxies(selector_name, name)?;
            }
            Action::ProxyDetailRefresh(focused) => {
                if let Some(detail_focused) = self.detail_focused
                    && detail_focused == focused
                {
                    return Ok(self.proxy_detail_action());
                }
            }
            Action::ProxiesRefresh => self.refresh_proxies()?,
            Action::Tick => {
                if self.loading.load(Ordering::Relaxed) {
                    self.throbber.calc_next();
                }
                if self.pending_test.load(Ordering::Relaxed) > 0 {
                    self.pending_test_throbber.calc_next();
                }
            }
            Action::ProxyTestRequest(name) => self.test_proxy(name, false)?,
            Action::ProxyGroupTestRequest(name) => self.test_proxy(name, true)?,
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
