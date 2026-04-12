use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use throbber_widgets_tui::{BLACK_CIRCLE, BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId};
use crate::models::proxy::Proxy;
use crate::store::proxies::Proxies;
use crate::store::proxy_setting::ProxySetting;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT, popup_area, space_between};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const CARD_HEIGHT: u16 = 3;
const CARD_WIDTH: u16 = 25;

#[derive(Debug, Default)]
pub struct ProxyDetailComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,

    show: bool,
    proxy_name: Option<String>,
    /// Proxy group navigation stack (breadcrumbs).
    ///
    /// - first: top-level proxy group
    /// - last:  currently viewed proxy group
    layers: Vec<Layer>,

    navigator: ScrollableNavigator,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,

    pending_test: Arc<AtomicU16>,
    pending_test_throbber: ThrobberState,
}

#[derive(Debug)]
struct Layer {
    name: String,
    navigator: ScrollableNavigator,
}

impl ProxyDetailComponent {
    pub fn show(&mut self, proxy_name: String) {
        debug!("Show proxy detail: {}", proxy_name);
        if Proxies::get_by_name(&proxy_name).is_none() {
            error!("Proxy not found: {}", proxy_name);
            self.close();
            return;
        };

        self.proxy_name = Some(proxy_name.clone());
        self.loading.store(false, Ordering::Relaxed);
        self.pending_test.store(0, Ordering::Relaxed);
        self.sync_layer(proxy_name);

        self.show = true;
    }

    pub fn hide(&mut self) {
        self.show = false;
        self.proxy_name = None;
        self.layers.clear();
    }

    fn close(&mut self) {
        self.hide();
        let _ = self.action_tx.as_ref().unwrap().send(Action::Unfocus);
    }

    /// Sync navigation stack with proxy name:
    /// - exists: navigate back → restore navigator
    /// - not exists: navigate into child → push new layer & reset navigator
    fn sync_layer(&mut self, name: String) {
        if self.layers.iter().any(|l| l.name == name) {
            // restore navigator state if navigating back
            self.navigator = self.layers.last().unwrap().navigator.clone();
        } else {
            // push new layer when navigating into a child proxy group
            self.layers.push(Layer { name, navigator: Default::default() });
            // reset navigator for new layer
            self.navigator.focused = None;
            self.navigator.scroller.position(0);
        }
    }

    fn backup_navigator(&mut self) {
        if let Some(layer) = self.layers.last_mut() {
            layer.navigator = self.navigator.clone();
        }
    }

    fn load_proxies(&mut self) -> Result<()> {
        self.loading.store(true, Ordering::Relaxed);
        info!("Loading proxies");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let loading = Arc::clone(&self.loading);
        let action_tx = self.action_tx.as_ref().unwrap().clone();

        tokio::task::Builder::new().name("proxies-loader").spawn(async move {
            if let Err(e) = Proxies::load(api).await {
                error!(error = ?e, "Failed to load proxies");
                let _ = action_tx.send(Action::Error(("Load proxy", e).into()));
            }
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn update_proxy(&mut self, selector_name: String, name: String) -> Result<()> {
        info!("Updating proxy {}: {}", selector_name, name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let loading = Arc::clone(&self.loading);
        let action_tx = self.action_tx.as_ref().unwrap().clone();

        tokio::task::Builder::new().name("proxy-updater").spawn(async move {
            if let Err(e) = Proxies::update_and_reload(api, &selector_name, &name).await {
                warn!(error = ?e, "Failed to update selected proxy for {}: {}", selector_name, name);
                let _ = action_tx.send(Action::Error(("Update selected proxy", e).into()));
            }
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn test_proxy(&self, name: String) -> Result<()> {
        info!("Testing proxy {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let pending_test = Arc::clone(&self.pending_test);
        pending_test.fetch_add(1, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-tester").spawn(async move {
            if let Err(e) = Proxies::test_and_reload(api, &name).await {
                error!(error = ?e, "Failed to test and load proxy: {}", name);
            }
            let _ = pending_test.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |x| {
                if x == 0 { None } else { Some(x - 1) }
            });
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
            // group already includes all child proxies,
            // so we can safely reset the count instead of decrementing it.
            pending_test.store(0, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn focus_current(&mut self, proxy: &Proxy) {
        let Some(current_sel) = proxy.selected.as_deref() else {
            return;
        };
        info!("Focus current proxy: {}", current_sel);
        if let Some(idx) =
            proxy.children.as_ref().and_then(|v| v.iter().position(|name| name == current_sel))
        {
            self.navigator.focus(idx);
        }
    }

    fn title_line(&'_ self, children_len: usize) -> Line<'_> {
        let names = self.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>();
        Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::styled(names.join(" > "), Color::White),
            Span::raw(" ("),
            Span::styled(format!("{}", children_len), Color::LightCyan),
            Span::raw(")"),
            Span::raw(TOP_TITLE_RIGHT),
        ])
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

    fn render_card(
        threshold: (u64, u64),
        group: &Proxy,
        proxy: &Proxy,
        focused: bool,
        frame: &mut Frame,
        area: Rect,
    ) {
        let selected = group.selected.as_deref().is_some_and(|v| v == proxy.name);
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

        let para = Paragraph::new(space_between(
            area.width - 2, // minus border
            Span::raw(proxy.r#type.as_str()),
            proxy.latency.as_span(threshold),
        ))
        .block(block);
        frame.render_widget(para, area);
    }

    fn render_cards(&mut self, group: &Proxy, frame: &mut Frame, area: Rect) {
        let children_names = group.children.as_deref().unwrap_or_default();
        let cols = (area.width / CARD_WIDTH).max(1) as usize;
        let col_chunks =
            Layout::horizontal((0..cols).map(|_| Constraint::Min(CARD_WIDTH))).split(area);
        self.navigator
            .step(cols)
            .length(children_names.len(), ((area.height / CARD_HEIGHT) as usize) * cols);
        let visible_names =
            &children_names[self.navigator.scroller.pos()..self.navigator.scroller.end_pos()];
        let threshold = ProxySetting::global().read().unwrap().threshold;
        Proxies::with_by_names(visible_names, |proxies| {
            self.navigator.iter_layout(proxies, CARD_HEIGHT, col_chunks).for_each(
                |(proxy, focused, rect)| {
                    Self::render_card(threshold, group, proxy, focused, frame, rect)
                },
            )
        });
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
            Shortcut::new(vec![
                Fragment::hl("PgUp"),
                Fragment::raw(" page "),
                Fragment::hl("PgDn"),
            ]),
            Shortcut::new(vec![Fragment::hl("g"), Fragment::raw(" jump "), Fragment::hl("G")]),
            Shortcut::new(vec![Fragment::hl("["), Fragment::raw(" layer "), Fragment::hl("]")]),
            Shortcut::from("cur", 0).unwrap(),
            Shortcut::new(vec![Fragment::raw("sel "), Fragment::hl("↵")]),
            Shortcut::new(vec![Fragment::raw("back "), Fragment::hl("Esc")]),
            Shortcut::from("refresh", 0).unwrap(),
            Shortcut::from("test", 0).unwrap(),
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
        let Some(proxy) = self.proxy_name.as_ref().and_then(|n| Proxies::get_by_name(n)) else {
            return Ok(None);
        };
        if self.navigator.handle_key_event(true, key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('c') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus_current(&proxy);
                return Ok(None);
            }
            KeyCode::Char('q') => self.close(),
            KeyCode::Esc => {
                if self.navigator.focused.is_some() {
                    self.navigator.focused = None;
                } else {
                    self.close();
                }
            }
            KeyCode::Char('r') => {
                self.backup_navigator();
                self.load_proxies()?;
            }
            KeyCode::Enter => {
                // update selected proxy
                if let Some(idx) = self.navigator.focused
                    && let Some(name) = proxy.children.as_ref().and_then(|v| v.get(idx))
                {
                    let selector_name = proxy.name.clone();
                    self.backup_navigator();
                    self.update_proxy(selector_name, name.clone())?;
                }
            }
            KeyCode::Char('t') => match self.navigator.focused {
                None => {
                    self.test_proxy_group(proxy.name.clone())?;
                }
                Some(idx) => {
                    if let Some(name) = proxy.children.as_ref().and_then(|v| v.get(idx).cloned()) {
                        self.test_proxy(name)?;
                    }
                }
            },
            KeyCode::Char('[')
                if !self.loading.load(Ordering::Relaxed) && self.layers.len() > 1 =>
            {
                // pop current layer
                self.layers.pop();
                // unwrap is safe because layers.len() > 1
                let parent_name = self.layers.last().map(|l| l.name.clone()).unwrap();
                self.show(parent_name);
            }
            KeyCode::Char(']') if !self.loading.load(Ordering::Relaxed) => {
                // Use `navigator.focused` first; otherwise fall back to the stored selection.
                let proxy_name = match self.navigator.focused {
                    Some(idx) => proxy.children.as_ref().and_then(|v| v.get(idx)),
                    None => proxy.selected.as_ref(),
                };
                if let Some(proxy) = proxy_name
                    .map(String::as_str)
                    .and_then(Proxies::get_by_name)
                    .filter(|p| p.children.as_ref().is_some_and(|c| !c.is_empty()))
                {
                    // Save current focus index before navigating
                    if let Some(layer) = self.layers.last_mut() {
                        layer.navigator = self.navigator.clone();
                    }
                    self.show(proxy.name.clone());
                }
            }
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::ProxyDetail(name) => self.show(name),
            Action::Tick if self.loading.load(Ordering::Relaxed) => {
                self.throbber.calc_next();
            }
            _ => (),
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.show || self.proxy_name.is_none() {
            return Ok(());
        }

        let proxy = match Proxies::get_by_name(self.proxy_name.as_ref().unwrap()) {
            None => {
                error!("Proxy not found: {}", self.proxy_name.as_ref().unwrap());
                self.close();
                return Ok(());
            }
            Some(p) => p,
        };

        let area = popup_area(area, 80, 80);
        frame.render_widget(Clear, area); // clears out the background
        // outer margin
        let area = area.inner(Margin::new(2, 1));

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(self.title_line(proxy.children.as_ref().map(Vec::len).unwrap_or_default()));
        let content_area = block.inner(area);
        frame.render_widget(block, area);
        self.render_throbber(frame, area);

        self.render_cards(&proxy, frame, content_area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
