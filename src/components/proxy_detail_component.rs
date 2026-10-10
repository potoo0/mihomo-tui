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
use crate::components::{Component, ComponentId, MIN_CARD_WIDTH};
use crate::config::LatencyThreshold;
use crate::models::proxy::Proxy;
use crate::render::RenderRequester;
use crate::store::proxies::Proxies;
use crate::store::proxy_detail_setting::ProxyDetailSetting;
use crate::store::proxy_setting::ProxySetting;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT, popup_area, space_between};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const CARD_HEIGHT: u16 = 3;
const CARD_WIDTH: u16 = 25;
const WIDTH_HINT_TICKS: u8 = 4;
const CARD_WIDTH_SAVE_TICKS: u8 = 4;

fn card_layout(area: Rect, preferred_width: u16) -> std::rc::Rc<[Rect]> {
    let width = preferred_width.max(MIN_CARD_WIDTH).min(area.width);
    let cols = (area.width / width.max(1)).max(1);
    Layout::horizontal((0..cols).map(|_| Constraint::Min(width))).split(area)
}

#[derive(Debug, Default)]
pub struct ProxyDetailComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,
    render_requester: Option<RenderRequester>,

    show: bool,
    proxy_name: Option<String>,
    /// Proxy group navigation stack (breadcrumbs).
    ///
    /// - first: top-level proxy group
    /// - last:  currently viewed proxy group
    layers: Vec<Layer>,

    navigator: ScrollableNavigator,
    card_area_width: u16,
    width_hint_ticks: u8,
    card_width_save_ticks: u8,

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
    fn adjust_card_width(&mut self, delta: i16) {
        if self.card_area_width == 0 {
            return;
        }
        let max_width = self.card_area_width.max(MIN_CARD_WIDTH);
        let previous = ProxyDetailSetting::snapshot().card_width.unwrap_or(CARD_WIDTH);
        let current = previous.min(max_width);
        let width = current.saturating_add_signed(delta).clamp(MIN_CARD_WIDTH, max_width);
        self.width_hint_ticks = WIDTH_HINT_TICKS;
        if previous != width {
            ProxyDetailSetting::update(|setting| setting.card_width = Some(width));
            self.card_width_save_ticks = CARD_WIDTH_SAVE_TICKS;
        }
    }

    fn notify_card_width_save(&self) {
        if let Some(tx) = &self.action_tx {
            let _ = tx.send(Action::ProxyDetailLayoutChanged);
        }
    }

    fn tick_card_width_save(&mut self) {
        if self.card_width_save_ticks == 0 {
            return;
        }
        self.card_width_save_ticks -= 1;
        if self.card_width_save_ticks == 0 {
            self.notify_card_width_save();
        }
    }

    fn flush_card_width_save(&mut self) {
        if self.card_width_save_ticks == 0 {
            return;
        }
        self.card_width_save_ticks = 0;
        self.notify_card_width_save();
    }

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
        self.width_hint_ticks = 0;
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
        let render_requester = self.render_requester.as_ref().unwrap().clone();

        tokio::task::Builder::new().name("proxies-loader").spawn(async move {
            if let Err(e) = Proxies::load(api).await {
                error!(error = ?e, "Failed to load proxies");
                let _ = action_tx.send(Action::Error(("Load proxy", e).into()));
            }
            loading.store(false, Ordering::Relaxed);
            render_requester.request_render();
        })?;

        Ok(())
    }

    fn update_proxy(&mut self, selector_name: String, name: String) -> Result<()> {
        info!("Updating proxy {}: {}", selector_name, name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let loading = Arc::clone(&self.loading);
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        let render_requester = self.render_requester.as_ref().unwrap().clone();

        tokio::task::Builder::new().name("proxy-updater").spawn(async move {
            match Proxies::update_and_reload(api.clone(), &selector_name, &name).await {
                Ok(()) => Self::spawn_connection_terminator(api, selector_name),
                Err(e) => {
                    warn!(error = ?e, "Failed to update selected proxy for {}: {}", selector_name, name);
                    let _ = action_tx.send(Action::Error(("Update selected proxy", e).into()));
                }
            }

            loading.store(false, Ordering::Relaxed);
            render_requester.request_render();
        })?;

        Ok(())
    }

    fn test_proxy(&self, name: String, is_group: bool, reset_pending: bool) -> Result<()> {
        info!(name = %name, is_group, reset_pending, "Testing proxy");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let pending_test = Arc::clone(&self.pending_test);
        let render_requester = self.render_requester.as_ref().unwrap().clone();
        pending_test.fetch_add(1, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-tester").spawn(async move {
            let result = if is_group {
                Proxies::test_group_and_reload(api, &name).await
            } else {
                Proxies::test_and_reload(api, &name).await
            };
            if let Err(e) = result {
                error!(error = ?e, name = %name, is_group, "Failed to test and load proxy");
            }
            if reset_pending {
                pending_test.store(0, Ordering::Relaxed);
            } else {
                pending_test.update(Ordering::Relaxed, Ordering::Relaxed, |n| n.saturating_sub(1));
            }
            render_requester.request_render();
        })?;

        Ok(())
    }

    fn spawn_connection_terminator(api: Arc<Api>, selector_name: String) {
        if !ProxySetting::global().read().unwrap().auto_terminate_connections {
            return;
        }
        debug!("Auto-terminating connections for selector {} after proxy update", selector_name);
        if let Err(e) = tokio::task::Builder::new().name("conn-terminator").spawn(async move {
            let Ok(wrapper) = api.get_connections().await else {
                debug!("Failed to get connections for termination");
                return;
            };
            // `into_iter + collect` to release large connection payloads early.
            let conns = wrapper
                .connections
                .into_iter()
                .flat_map(|c| c.into_iter())
                .filter(|c| c.chains.contains(&selector_name))
                .map(|c| c.id)
                .collect::<Vec<_>>();
            debug!(selector_name = %selector_name, num_conns = conns.len(), "Terminating connections");
            for conn_id in conns {
                if let Err(e) = api.delete_connection(&conn_id).await {
                    debug!(error = ?e, "Failed to terminate connection: {}", conn_id);
                }
            }
        }) {
            warn!(error = ?e, "Failed to spawn connection terminator task");
        }
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

    fn with_width_hint<'a>(&self, block: Block<'a>, content_area: Rect) -> Block<'a> {
        if self.width_hint_ticks == 0 {
            return block;
        }
        let preferred_width = ProxyDetailSetting::snapshot().card_width.unwrap_or(CARD_WIDTH);
        let actual_width =
            card_layout(content_area, preferred_width).first().map_or(0, |card| card.width);
        block.title_bottom(
            Line::styled(
                crate::i18n::messages::card_width(preferred_width, actual_width),
                Color::LightCyan,
            )
            .centered(),
        )
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if self.pending_test.load(Ordering::Relaxed) > 0 {
            let symbol = Throbber::default()
                .label(crate::i18n::tr("Testing"))
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
                .label(crate::i18n::tr("Loading"))
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
        threshold: LatencyThreshold,
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
        self.card_area_width = area.width;
        let preferred_width = ProxyDetailSetting::snapshot().card_width.unwrap_or(CARD_WIDTH);
        let col_chunks = card_layout(area, preferred_width);
        let cols = col_chunks.len();
        let previous_cols = self.navigator.scroller.step_value();
        self.navigator
            .step(cols)
            .length(children_names.len(), ((area.height / CARD_HEIGHT) as usize) * cols);
        // Keep the focused card visible and the scroll position row-aligned when columns change.
        if cols != previous_cols {
            let anchor = self.navigator.focused.unwrap_or(self.navigator.scroller.pos());
            let last_row = children_names
                .len()
                .saturating_sub(self.navigator.scroller.viewport_content_length())
                .div_ceil(cols)
                * cols;
            self.navigator.scroller.position(((anchor / cols) * cols).min(last_row));
        }
        let visible_names =
            &children_names[self.navigator.scroller.pos()..self.navigator.scroller.end_pos()];
        let threshold = ProxySetting::global().read().unwrap().latency_threshold;
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
            Shortcut::new(vec![Fragment::hl("q"), Fragment::raw("/"), Fragment::hl("Esc")]),
            Shortcut::new(vec![
                Fragment::hl(arrow::LEFT),
                Fragment::raw("/"),
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
                Fragment::raw("/"),
                Fragment::hl(arrow::RIGHT),
            ])
            .compact(vec![
                Fragment::hl(arrow::LEFT),
                Fragment::raw("/"),
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
                Fragment::raw("/"),
                Fragment::hl(arrow::RIGHT),
            ]),
            Shortcut::new(vec![
                Fragment::hl("s"),
                Fragment::raw("/"),
                Fragment::hl("S"),
                Fragment::raw("ort"),
            ]),
            Shortcut::new(vec![Fragment::hl("["), Fragment::raw(" layer "), Fragment::hl("]")])
                .compact(vec![Fragment::hl("[/]"), Fragment::raw(" layer")]),
            Shortcut::from("cur", 0).unwrap(),
            Shortcut::new(vec![Fragment::raw("sel "), Fragment::hl("↵")]),
            Shortcut::new(vec![
                Fragment::hl("-"),
                Fragment::raw("/"),
                Fragment::hl("+"),
                Fragment::raw(" width"),
            ])
            .compact(vec![
                Fragment::hl("-"),
                Fragment::raw("/"),
                Fragment::hl("+"),
                Fragment::raw(" w"),
            ]),
            Shortcut::new(vec![Fragment::raw("back "), Fragment::hl("Esc")]),
            Shortcut::from("test", 0).unwrap(),
            Shortcut::from("refresh", 0).unwrap(),
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

    fn register_render_requester(&mut self, requester: RenderRequester) -> Result<()> {
        self.render_requester = Some(requester);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        let Some(proxy) = self.proxy_name.as_ref().and_then(|n| Proxies::get_by_name(n)) else {
            return Ok(None);
        };
        if self.navigator.handle_key_event(true, key).is_consumed() {
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
            KeyCode::Char('-') if key.modifiers == KeyModifiers::NONE => self.adjust_card_width(-1),
            KeyCode::Char('=') if key.modifiers == KeyModifiers::NONE => self.adjust_card_width(1),
            KeyCode::Char('+')
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.adjust_card_width(1);
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
            KeyCode::Char('t') => {
                let (name, is_group, reset_pending) = self
                    .navigator
                    .focused
                    .and_then(|idx| proxy.children.as_ref().and_then(|v| v.get(idx)))
                    .map(|name| {
                        let is_group = Proxies::get_by_name(name)
                            .map(|p| p.children.as_ref().is_some_and(|c| !c.is_empty()))
                            .unwrap_or(false);
                        (name.clone(), is_group, false)
                    })
                    .unwrap_or_else(|| (proxy.name.clone(), proxy.children.is_some(), true));
                self.test_proxy(name, is_group, reset_pending)?;
            }
            KeyCode::Char('s') => Proxies::switch_sort_field(
                self.api.clone().unwrap(),
                self.render_requester.as_ref().unwrap().clone(),
            ),
            KeyCode::Char('S') => Proxies::toggle_sort_direction(
                self.api.clone().unwrap(),
                self.render_requester.as_ref().unwrap().clone(),
            ),
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
            Action::Quit => self.flush_card_width_save(),
            Action::Tick => {
                self.width_hint_ticks = self.width_hint_ticks.saturating_sub(1);
                self.tick_card_width_save();
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
        let block = self.with_width_hint(block, content_area);
        frame.render_widget(block, area);
        self.render_throbber(frame, area);

        self.render_cards(&proxy, frame, content_area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    #[test]
    fn card_layout_respects_width_limits() {
        let area = Rect::new(2, 3, 80, 6);
        let narrow = card_layout(area, 10);
        assert_eq!(narrow.len(), 4);
        assert_eq!(narrow[0].width, 20);
        assert_eq!(narrow[1].x, 22);

        let stretched = card_layout(Rect::new(0, 0, 92, 6), 40);
        assert_eq!(stretched.len(), 2);
        assert_eq!(stretched[0].width, 46);
        assert_eq!(stretched[1].width, 46);

        let wide = card_layout(area, 100);
        assert_eq!(wide.len(), 1);
        assert_eq!(wide[0].width, area.width);

        let tiny = card_layout(Rect::new(0, 0, 12, 3), CARD_WIDTH);
        assert_eq!(tiny.len(), 1);
        assert_eq!(tiny[0].width, 12);
    }

    #[test]
    fn card_width_save_waits_four_ticks_after_last_change() {
        let previous = ProxyDetailSetting::snapshot();
        ProxyDetailSetting::update(|setting| setting.card_width = None);
        let (tx, mut rx) = unbounded_channel();
        let mut component =
            ProxyDetailComponent { action_tx: Some(tx), card_area_width: 80, ..Default::default() };

        component.adjust_card_width(1);
        component.update(Action::Tick).unwrap();
        component.adjust_card_width(1);
        assert_eq!(ProxyDetailSetting::snapshot().card_width, Some(27));
        for _ in 0..3 {
            component.update(Action::Tick).unwrap();
            assert!(rx.try_recv().is_err());
        }
        component.update(Action::Tick).unwrap();
        assert!(matches!(rx.try_recv(), Ok(Action::ProxyDetailLayoutChanged)));
        ProxyDetailSetting::update(|setting| *setting = previous);
    }

    #[test]
    fn quit_flushes_pending_card_width_save() {
        let (tx, mut rx) = unbounded_channel();
        let mut component = ProxyDetailComponent {
            action_tx: Some(tx),
            card_width_save_ticks: CARD_WIDTH_SAVE_TICKS,
            ..Default::default()
        };
        component.update(Action::Quit).unwrap();
        assert!(matches!(rx.try_recv(), Ok(Action::ProxyDetailLayoutChanged)));
    }
}
