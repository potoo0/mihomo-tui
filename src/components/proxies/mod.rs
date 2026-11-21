mod detail;
mod inspect;
mod settings;

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
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub(super) use self::detail::ProxyDetailComponent;
pub(super) use self::inspect::ProxyInspectComponent;
pub(super) use self::settings::ProxySettingComponent;
use crate::api::Api;
use crate::app::action::Action;
use crate::components::{Component, ComponentContext, ComponentId, ComponentIdentity};
use crate::config::LatencyThreshold;
use crate::shortcut;
use crate::store::hub::StoreRef;
use crate::store::proxy::{Proxies, ProxyDetailSetting, ProxySetting, ProxyView};
use crate::ui::layout::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::ui::render::RenderRequester;
use crate::ui::scrollable_navigator::ScrollableNavigator;
use crate::ui::shortcut::{Fragment, Shortcut};
use crate::ui::symbols::{arrow, key};

const CARD_HEIGHT: u16 = 4;
const CARDS_PER_ROW: usize = 2;

#[derive(Clone, Debug)]
struct ProxyStores {
    proxies: StoreRef<Proxies>,
    proxy_setting: StoreRef<ProxySetting>,
    detail_setting: StoreRef<ProxyDetailSetting>,
}

impl ProxyStores {
    fn new(ctx: &ComponentContext) -> Result<Self> {
        let detail_setting = ctx.stores.get_or_register(|| {
            Ok(ctx
                .config
                .ui
                .as_ref()
                .and_then(|ui| ui.proxy_detail.as_ref())
                .map(ProxyDetailSetting::from)
                .unwrap_or_default())
        })?;

        Ok(Self {
            proxies: ctx.stores.get_or_register(|| Ok(Proxies::new(detail_setting.clone())))?,
            proxy_setting: ctx.stores.get_or_register(|| Ok(ctx.config.proxy_setting.clone()))?,
            detail_setting,
        })
    }
}

#[derive(Debug)]
pub struct ProxiesComponent {
    api: Arc<Api>,
    render_requester: RenderRequester,
    stores: ProxyStores,
    token: CancellationToken,

    navigator: ScrollableNavigator,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,

    pending_test: Arc<AtomicU16>,
    pending_test_throbber: ThrobberState,
}

impl ProxiesComponent {
    pub fn new(ctx: &ComponentContext) -> Result<Self> {
        let shared_store = ProxyStores::new(ctx)?;

        Ok(Self {
            api: Arc::clone(&ctx.api),
            render_requester: ctx.render.clone(),
            stores: shared_store,
            token: CancellationToken::new(),

            navigator: ScrollableNavigator::new(CARDS_PER_ROW),
            loading: Default::default(),
            throbber: Default::default(),
            pending_test: Default::default(),
            pending_test_throbber: Default::default(),
        })
    }

    fn load_proxies(&mut self) -> Result<()> {
        self.loading.store(true, Ordering::Relaxed);
        info!("Loading proxies");
        let api = Arc::clone(&self.api);
        let loading = Arc::clone(&self.loading);
        let render_requester = self.render_requester.clone();
        let stores = self.stores.clone();
        let token = self.token.clone();

        tokio::task::Builder::new().name("proxies-loader").spawn(async move {
            let result = tokio::select! {
                _ = token.cancelled() => None,
                result = stores.proxies.load(api, &stores.proxy_setting) => Some(result),
            };

            if let Some(Err(e)) = result {
                error!(error = ?e, "Failed to load proxies");
            }
            loading.store(false, Ordering::Relaxed);
            render_requester.request_render();
        })?;

        Ok(())
    }

    fn test_proxy_group(&self, name: String) -> Result<()> {
        info!("Testing proxy group {}", name);
        let api = Arc::clone(&self.api);
        let pending_test = Arc::clone(&self.pending_test);
        let render_requester = self.render_requester.clone();
        let stores = self.stores.clone();
        let token = self.token.clone();
        pending_test.fetch_add(1, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-group-tester").spawn(async move {
            let result = tokio::select! {
                _ = token.cancelled() => None,
                result = stores.proxies.test_group_and_reload(api, &stores.proxy_setting, &name) => {
                    Some(result)
                }
            };

            if let Some(Err(e)) = result {
                error!(error = ?e, "Failed to test and load proxy: {}", name);
            }
            pending_test.update(Ordering::Relaxed, Ordering::Relaxed, |n| n.saturating_sub(1));
            render_requester.request_render();
        })?;

        Ok(())
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if self.pending_test.load(Ordering::Relaxed) > 0 {
            let symbol = Throbber::default()
                .label(rust_i18n::t!("status.testing"))
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
                .label(rust_i18n::t!("status.loading"))
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
        threshold: LatencyThreshold,
        view: &ProxyView,
        focused: bool,
        frame: &mut Frame,
        area: Rect,
    ) {
        let title_line = Line::from(vec![
            Span::styled(view.proxy.name.as_str(), Color::Reset),
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
        let proxies_len = self.stores.proxies.read(|store| store.with_view(|view| view.len()));
        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw(rust_i18n::t!("tabs.proxies")),
            Span::raw(" ("),
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
        let proxies = self.stores.proxies.read(|store| {
            store.with_view(|records| {
                records
                    .get(self.navigator.scroller.pos()..self.navigator.scroller.end_pos())
                    .map(|slice| slice.to_vec())
                    .unwrap_or_default()
            })
        });
        let threshold = self.stores.proxy_setting.read(|setting| setting.latency_threshold);
        self.navigator.iter_layout(&proxies, CARD_HEIGHT, col_chunks).for_each(
            |(proxy, focused, rect)| {
                Self::render_proxy(threshold, proxy, focused, frame, rect);
            },
        );
    }
}

impl Drop for ProxiesComponent {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

impl ComponentIdentity for ProxiesComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Proxies
    }
}

impl Component for ProxiesComponent {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::key(arrow::LEFT),
                Fragment::sep("/"),
                Fragment::key(arrow::UP),
                Fragment::sep("/"),
                Fragment::key("PgUp"),
                Fragment::sep("/"),
                Fragment::key("g"),
                Fragment::label("shortcuts.nav"),
                Fragment::key("G"),
                Fragment::sep("/"),
                Fragment::key("PgDn"),
                Fragment::sep("/"),
                Fragment::key(arrow::DOWN),
                Fragment::sep("/"),
                Fragment::key(arrow::RIGHT),
            ]),
            Shortcut::new(vec![Fragment::key(key::ENTER), Fragment::label("shortcuts.detail")]),
            shortcut!("inspect", 0),
            shortcut!("refresh", 0),
            shortcut!("setting", 0),
            shortcut!("test", 0),
        ]
    }

    fn start(&mut self) -> Result<()> {
        self.load_proxies()
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.navigator.handle_key_event(true, key).is_consumed() {
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc => self.navigator.focused = None,
            KeyCode::Char('r') => self.load_proxies()?,
            KeyCode::Char('s') => return Ok(Some(Action::ProxySetting)),
            KeyCode::Char('i') => {
                let action = self
                    .navigator
                    .focused
                    .and_then(|index| self.stores.proxies.read(|store| store.get(index)))
                    .map(|proxy| Action::ProxyInspect(Arc::clone(&proxy.proxy)));
                return Ok(action);
            }
            KeyCode::Enter => {
                let action = self
                    .navigator
                    .focused
                    .and_then(|index| self.stores.proxies.read(|store| store.get(index)))
                    .map(|v| Action::ProxyDetail(v.proxy.name.clone()));
                return Ok(action);
            }
            KeyCode::Char('t') => {
                if let Some(name) = self
                    .navigator
                    .focused
                    .and_then(|index| self.stores.proxies.read(|store| store.get(index)))
                    .map(|v| v.proxy.name.clone())
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

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use indexmap::IndexMap;
    use serde_json::json;

    use super::*;
    use crate::store::hub::StoreHub;

    fn proxy(name: &str, children: Option<Vec<&str>>) -> crate::models::proxy::Proxy {
        serde_json::from_value(json!({
            "name": name,
            "type": "Selector",
            "all": children,
            "history": []
        }))
        .unwrap()
    }

    #[test]
    fn component_registers_shared_proxy_stores() {
        let stores = StoreHub::default();
        let config = Arc::new(crate::config::default_config().unwrap());
        let _component = ProxiesComponent::new(&ComponentContext {
            config,
            stores: stores.clone(),
            ..ComponentContext::test_ctx()
        })
        .unwrap();

        assert!(stores.get::<Proxies>().is_ok());
        assert!(stores.get::<ProxySetting>().is_ok());
    }

    #[test]
    fn cloned_contexts_share_stores_and_component_recreation_retains_them() {
        let ctx = ComponentContext::test_ctx();
        let first = ProxiesComponent::new(&ctx).unwrap();
        let proxies = IndexMap::from([("node".to_string(), proxy("node", None))]);
        let threshold = first.stores.proxy_setting.read(|setting| setting.latency_threshold);
        first.stores.proxies.update(|store| store.push(proxies, threshold));

        let second = ProxiesComponent::new(&ctx.clone()).unwrap();
        first.stores.proxy_setting.update(|setting| {
            setting.test_url = "https://shared.example/generate_204".into();
        });
        assert_eq!(
            second.stores.proxy_setting.read(|setting| setting.test_url.clone()),
            "https://shared.example/generate_204"
        );

        drop(first);
        drop(second);
        let recreated = ProxiesComponent::new(&ctx).unwrap();
        assert!(recreated.stores.proxies.read(|store| store.get_by_name("node").is_some()));
        assert_eq!(
            recreated.stores.proxy_setting.read(|setting| setting.test_url.clone()),
            "https://shared.example/generate_204"
        );
    }

    #[test]
    fn independent_contexts_do_not_share_proxy_settings() {
        let left = ProxiesComponent::new(&ComponentContext::test_ctx()).unwrap();
        let right = ProxiesComponent::new(&ComponentContext::test_ctx()).unwrap();
        let original = right.stores.proxy_setting.snapshot();

        left.stores.proxy_setting.update(|setting| {
            setting.test_url = "https://left.example/generate_204".into();
        });

        assert_eq!(
            right.stores.proxy_setting.read(|setting| setting.test_url.clone()),
            original.test_url
        );
    }

    #[test]
    fn i_inspects_focused_proxy() {
        let stores = StoreHub::default();
        let detail_setting = stores.register(ProxyDetailSetting::default()).unwrap();
        let proxies_store = stores.register(Proxies::new(detail_setting.clone())).unwrap();
        let proxy_setting = stores.register(ProxySetting::default()).unwrap();
        let proxies = IndexMap::from([
            ("group".to_string(), proxy("group", Some(vec!["node"]))),
            ("node".to_string(), proxy("node", None)),
        ]);
        let threshold = proxy_setting.read(|setting| setting.latency_threshold);
        proxies_store.update(|store| store.push(proxies, threshold));

        let mut component = ProxiesComponent::new(&ComponentContext::test_ctx()).unwrap();
        component.stores = ProxyStores { proxies: proxies_store, proxy_setting, detail_setting };
        component.navigator.focused = Some(0);
        let action = component
            .handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
            .unwrap();

        match action {
            Some(Action::ProxyInspect(proxy)) => assert_eq!(proxy.name, "group"),
            other => panic!("unexpected action: {other:?}"),
        }
    }
}
