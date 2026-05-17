use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::{Color, Line, Span, Style};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use throbber_widgets_tui::{BLACK_CIRCLE, BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId};
use crate::models::proxy::Proxy;
use crate::store::proxy_providers::{ProviderView, ProxyProviders};
use crate::store::proxy_setting::ProxySetting;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT, popup_area, space_between};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const CARD_HEIGHT: u16 = 3;
const CARD_WIDTH: u16 = 25;

#[derive(Debug, Default)]
pub struct ProxyProviderDetailComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,

    show: bool,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,

    health_checking: Arc<AtomicBool>,
    health_checking_throbber: ThrobberState,

    provider_name: Option<String>,
    provider_index: Option<usize>,
    navigator: ScrollableNavigator,
}

impl ProxyProviderDetailComponent {
    pub fn show(&mut self, provider_name: String) {
        self.show = true;
        self.provider_name = Some(provider_name);
        self.navigator.focused = None;
        self.navigator.scroller.position(0);
    }

    pub fn hide(&mut self) {
        self.show = false;
        self.provider_name = None;
        self.provider_index = None;
    }

    fn close(&mut self) {
        self.hide();
        let _ = self.action_tx.as_ref().unwrap().send(Action::Unfocus);
    }

    fn load_providers(&self) -> anyhow::Result<()> {
        info!("Loading proxy providers");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let loading = Arc::clone(&self.loading);
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-providers-loader").spawn(async move {
            if let Err(e) = ProxyProviders::load(api).await {
                error!(error = ?e, "Failed to get proxy providers")
            }
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn provider_health_check(&self, name: String) -> anyhow::Result<()> {
        info!("Health check for provider: {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let health_checking = Arc::clone(&self.health_checking);
        health_checking.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-provider-health-check").spawn(async move {
            if let Err(e) = ProxyProviders::health_check_and_reload(api, &name).await {
                error!(error = ?e, "Failed to health check and reload provider");
            }
            health_checking.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn update_provider(&self, name: String) -> anyhow::Result<()> {
        info!("Update provider: {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        let loading = Arc::clone(&self.loading);
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-provider-update").spawn(async move {
            if let Err(e) = ProxyProviders::update_and_reload(api, &name).await {
                error!(error = ?e, "Failed to update provider");
                let _ = action_tx.send(Action::Error(("Update proxy provider", e).into()));
            }
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn title_line(provider_view: &'_ ProviderView) -> Line<'_> {
        let provider = &provider_view.provider;
        Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::styled(provider.name.as_str(), Color::White),
            Span::raw(" ("),
            Span::styled(format!("{}", provider.proxies.len()), Color::LightCyan),
            Span::raw(") - "),
            Span::raw(provider.vehicle_type.as_str()),
            Span::raw(TOP_TITLE_RIGHT),
        ])
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if self.health_checking.load(Ordering::Relaxed) {
            let symbol = Throbber::default()
                .label("Testing")
                .style(Style::default().fg(Color::White).bg(Color::Green).bold())
                .throbber_style(Style::default().fg(Color::White).bg(Color::Green).bold())
                .throbber_set(BLACK_CIRCLE)
                .use_type(WhichUse::Spin);
            frame.render_stateful_widget(
                symbol,
                Rect::new(area.right().saturating_sub(20), area.y, 9, 1),
                &mut self.health_checking_throbber,
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
        proxy: &Proxy,
        focused: bool,
        frame: &mut Frame,
        area: Rect,
    ) {
        let (border_type, border_color) = if focused {
            (BorderType::Thick, Color::Cyan)
        } else {
            (BorderType::Rounded, Color::DarkGray)
        };
        let block = Block::bordered()
            .border_type(border_type)
            .border_style(border_color)
            .title_top(Span::raw(proxy.name.as_str()));

        let para = Paragraph::new(space_between(
            area.width - 2, // minus border
            Span::raw(proxy.r#type.as_str()),
            proxy.latency.as_span(threshold),
        ))
        .block(block);
        frame.render_widget(para, area);
    }

    fn render_cards(&mut self, provider: &ProviderView, frame: &mut Frame, area: Rect) {
        let provider = &provider.provider;
        let cols = (area.width / CARD_WIDTH).max(1) as usize;
        let col_chunks =
            Layout::horizontal((0..cols).map(|_| Constraint::Min(CARD_WIDTH))).split(area);
        self.navigator
            .step(cols)
            .length(provider.proxies.len(), ((area.height / CARD_HEIGHT) as usize) * cols);
        let visible =
            &provider.proxies[self.navigator.scroller.pos()..self.navigator.scroller.end_pos()];
        let threshold = ProxySetting::global().read().unwrap().threshold;
        self.navigator.iter_layout(visible, CARD_HEIGHT, col_chunks).for_each(
            |(proxy, focused, rect)| Self::render_card(threshold, proxy, focused, frame, rect),
        );
    }

    fn get_provider(&mut self) -> Option<Arc<ProviderView>> {
        let provider_name = self.provider_name.as_deref()?;
        if let Some(provider) = self
            .provider_index
            .and_then(ProxyProviders::get)
            .filter(|p| p.provider.name == provider_name)
        {
            return Some(provider);
        }
        let (index, provider) = ProxyProviders::get_by_name(provider_name)?;
        self.provider_index = Some(index);
        Some(provider)
    }
}

impl Component for ProxyProviderDetailComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ProxyProviderDetail
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
            Shortcut::new(vec![Fragment::raw("back "), Fragment::hl("Esc")]),
            Shortcut::from("refresh", 0).unwrap(),
            Shortcut::from("test", 0).unwrap(),
            Shortcut::from("update", 0).unwrap(),
            Shortcut::from("sort", 0).unwrap(),
        ]
    }

    fn init(&mut self, api: Arc<Api>) -> anyhow::Result<()> {
        self.api = Some(api);
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> anyhow::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> anyhow::Result<Option<Action>> {
        let Some(provider_name) = self.provider_name.clone() else {
            return Ok(None);
        };
        if self.navigator.handle_key_event(true, key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Char('r') => self.load_providers()?,
            KeyCode::Char('t') => self.provider_health_check(provider_name)?,
            KeyCode::Char('u') => self.update_provider(provider_name)?,
            KeyCode::Char('s') => ProxyProviders::switch_sort_field(self.api.clone().unwrap()),
            KeyCode::Char('S') => ProxyProviders::toggle_sort_direction(self.api.clone().unwrap()),
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> anyhow::Result<Option<Action>> {
        match action {
            Action::ProxyProviderDetail(name) => self.show(name),
            Action::Tick => {
                if self.loading.load(Ordering::Relaxed) {
                    self.throbber.calc_next();
                }
                if self.health_checking.load(Ordering::Relaxed) {
                    self.health_checking_throbber.calc_next();
                }
            }
            _ => (),
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> anyhow::Result<()> {
        if !self.show {
            return Ok(());
        }

        let provider = match self.get_provider() {
            None => {
                error!("Proxy provider not found: {}", self.provider_name.as_ref().unwrap());
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
            .title(Self::title_line(&provider));
        let content_area = block.inner(area);
        frame.render_widget(block, area);
        self.render_throbber(frame, area);

        self.render_cards(&provider, frame, content_area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
