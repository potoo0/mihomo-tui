use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::{Color, Line, Span, Style};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use throbber_widgets_tui::{BLACK_CIRCLE, BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId, MIN_CARD_WIDTH};
use crate::config::LatencyThreshold;
use crate::models::proxy::Proxy;
use crate::render::RenderRequester;
use crate::store::proxy_provider_detail_setting::ProxyProviderDetailSetting;
use crate::store::proxy_providers::{ProviderView, ProxyProviders};
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
pub struct ProxyProviderDetailComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,
    render_requester: Option<RenderRequester>,

    show: bool,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,

    health_checking: Arc<AtomicBool>,
    health_checking_throbber: ThrobberState,

    provider_name: Option<String>,
    provider_index: Option<usize>,
    navigator: ScrollableNavigator,
    card_area_width: u16,
    width_hint_ticks: u8,
    card_width_save_ticks: u8,
}

impl ProxyProviderDetailComponent {
    fn adjust_card_width(&mut self, delta: i16) {
        if self.card_area_width == 0 {
            return;
        }
        let max_width = self.card_area_width.max(MIN_CARD_WIDTH);
        let previous = ProxyProviderDetailSetting::snapshot().card_width.unwrap_or(CARD_WIDTH);
        let current = previous.min(max_width);
        let width = current.saturating_add_signed(delta).clamp(MIN_CARD_WIDTH, max_width);
        self.width_hint_ticks = WIDTH_HINT_TICKS;
        if previous != width {
            ProxyProviderDetailSetting::update(|setting| setting.card_width = Some(width));
            self.card_width_save_ticks = CARD_WIDTH_SAVE_TICKS;
        }
    }

    fn notify_card_width_save(&self) {
        if let Some(tx) = &self.action_tx {
            let _ = tx.send(Action::ProxyProviderDetailLayoutChanged);
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
        self.width_hint_ticks = 0;
    }

    fn close(&mut self) {
        self.hide();
        let _ = self.action_tx.as_ref().unwrap().send(Action::Unfocus);
    }

    fn load_providers(&self) -> anyhow::Result<()> {
        info!("Loading proxy providers");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let loading = Arc::clone(&self.loading);
        let render_requester = self.render_requester.as_ref().unwrap().clone();
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-providers-loader").spawn(async move {
            if let Err(e) = ProxyProviders::load(api).await {
                error!(error = ?e, "Failed to get proxy providers")
            }
            loading.store(false, Ordering::Relaxed);
            render_requester.request_render();
        })?;

        Ok(())
    }

    fn provider_health_check(&self, name: String) -> anyhow::Result<()> {
        info!("Health check for provider: {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let health_checking = Arc::clone(&self.health_checking);
        let render_requester = self.render_requester.as_ref().unwrap().clone();
        health_checking.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-provider-health-check").spawn(async move {
            if let Err(e) = ProxyProviders::health_check_and_reload(api, &name).await {
                error!(error = ?e, "Failed to health check and reload provider");
            }
            health_checking.store(false, Ordering::Relaxed);
            render_requester.request_render();
        })?;

        Ok(())
    }

    fn update_provider(&self, name: String) -> anyhow::Result<()> {
        info!("Update provider: {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        let loading = Arc::clone(&self.loading);
        let render_requester = self.render_requester.as_ref().unwrap().clone();
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-provider-update").spawn(async move {
            if let Err(e) = ProxyProviders::update_and_reload(api, &name).await {
                error!(error = ?e, "Failed to update provider");
                let _ = action_tx.send(Action::Error(("Update proxy provider", e).into()));
            }
            loading.store(false, Ordering::Relaxed);
            render_requester.request_render();
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

    fn with_width_hint<'a>(&self, block: Block<'a>, content_area: Rect) -> Block<'a> {
        if self.width_hint_ticks == 0 {
            return block;
        }
        let preferred_width =
            ProxyProviderDetailSetting::snapshot().card_width.unwrap_or(CARD_WIDTH);
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
        if self.health_checking.load(Ordering::Relaxed) {
            let symbol = Throbber::default()
                .label(crate::i18n::tr("Testing"))
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
        self.card_area_width = area.width;
        let preferred_width =
            ProxyProviderDetailSetting::snapshot().card_width.unwrap_or(CARD_WIDTH);
        let col_chunks = card_layout(area, preferred_width);
        let cols = col_chunks.len();
        let previous_cols = self.navigator.scroller.step_value();
        self.navigator
            .step(cols)
            .length(provider.proxies.len(), ((area.height / CARD_HEIGHT) as usize) * cols);
        // Keep the focused card visible and the scroll position row-aligned when columns change.
        if cols != previous_cols {
            let anchor = self.navigator.focused.unwrap_or(self.navigator.scroller.pos());
            let last_row = provider
                .proxies
                .len()
                .saturating_sub(self.navigator.scroller.viewport_content_length())
                .div_ceil(cols)
                * cols;
            self.navigator.scroller.position(((anchor / cols) * cols).min(last_row));
        }
        let visible =
            &provider.proxies[self.navigator.scroller.pos()..self.navigator.scroller.end_pos()];
        let threshold = ProxySetting::global().read().unwrap().latency_threshold;
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
            ]),
            Shortcut::new(vec![
                Fragment::hl("s"),
                Fragment::raw("/"),
                Fragment::hl("S"),
                Fragment::raw("ort"),
            ]),
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
            Shortcut::from("update", 0).unwrap(),
            Shortcut::from("refresh", 0).unwrap(),
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

    fn register_render_requester(&mut self, requester: RenderRequester) -> anyhow::Result<()> {
        self.render_requester = Some(requester);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> anyhow::Result<Option<Action>> {
        let Some(provider_name) = self.provider_name.clone() else {
            return Ok(None);
        };
        if self.navigator.handle_key_event(true, key).is_consumed() {
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
            KeyCode::Char('-') if key.modifiers == KeyModifiers::NONE => self.adjust_card_width(-1),
            KeyCode::Char('=') if key.modifiers == KeyModifiers::NONE => self.adjust_card_width(1),
            KeyCode::Char('+')
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.adjust_card_width(1);
            }
            KeyCode::Char('s') => ProxyProviders::switch_sort_field(
                self.api.clone().unwrap(),
                self.render_requester.as_ref().unwrap().clone(),
            ),
            KeyCode::Char('S') => ProxyProviders::toggle_sort_direction(
                self.api.clone().unwrap(),
                self.render_requester.as_ref().unwrap().clone(),
            ),
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> anyhow::Result<Option<Action>> {
        match action {
            Action::ProxyProviderDetail(name) => self.show(name),
            Action::Quit => self.flush_card_width_save(),
            Action::Tick => {
                self.width_hint_ticks = self.width_hint_ticks.saturating_sub(1);
                self.tick_card_width_save();
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
        let block = self.with_width_hint(block, content_area);
        frame.render_widget(block, area);
        self.render_throbber(frame, area);

        self.render_cards(&provider, frame, content_area);
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

        let stretched = card_layout(Rect::new(0, 0, 92, 6), 40);
        assert_eq!(stretched.len(), 2);
        assert_eq!(stretched[0].width, 46);

        let wide = card_layout(area, 100);
        assert_eq!(wide.len(), 1);
        assert_eq!(wide[0].width, area.width);
    }

    #[test]
    fn card_width_save_waits_four_ticks_after_last_change() {
        let previous = ProxyProviderDetailSetting::snapshot();
        ProxyProviderDetailSetting::update(|setting| setting.card_width = None);
        let (tx, mut rx) = unbounded_channel();
        let mut component = ProxyProviderDetailComponent {
            action_tx: Some(tx),
            card_area_width: 80,
            ..Default::default()
        };

        component.adjust_card_width(1);
        component.update(Action::Tick).unwrap();
        component.adjust_card_width(1);
        assert_eq!(ProxyProviderDetailSetting::snapshot().card_width, Some(27));
        for _ in 0..3 {
            component.update(Action::Tick).unwrap();
            assert!(rx.try_recv().is_err());
        }
        component.update(Action::Tick).unwrap();
        assert!(matches!(rx.try_recv(), Ok(Action::ProxyProviderDetailLayoutChanged)));
        ProxyProviderDetailSetting::update(|setting| *setting = previous);
    }

    #[test]
    fn quit_flushes_pending_card_width_save() {
        let (tx, mut rx) = unbounded_channel();
        let mut component = ProxyProviderDetailComponent {
            action_tx: Some(tx),
            card_width_save_ticks: CARD_WIDTH_SAVE_TICKS,
            ..Default::default()
        };
        component.update(Action::Quit).unwrap();
        assert!(matches!(rx.try_recv(), Ok(Action::ProxyProviderDetailLayoutChanged)));
    }
}
