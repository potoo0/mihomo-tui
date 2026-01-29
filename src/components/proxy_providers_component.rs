use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols::bar;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use throbber_widgets_tui::{BLACK_CIRCLE, BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::proxy_providers::{ProviderView, ProxyProviders};
use crate::components::{Component, ComponentId};
use crate::utils::byte_size::human_bytes;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT, space_between_many};
use crate::utils::time::format_timestamp;
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const CARD_HEIGHT: u16 = 6;
const CARDS_PER_ROW: usize = 2;

#[derive(Debug, Default)]
pub struct ProxyProvidersComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,

    store: Arc<RwLock<ProxyProviders>>,
    navigator: ScrollableNavigator,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,

    pending_test: Arc<AtomicU16>,
    pending_test_throbber: ThrobberState,
}

impl ProxyProvidersComponent {
    fn load_providers(&self) -> Result<()> {
        info!("Loading proxy providers");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let loading = Arc::clone(&self.loading);
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-providers-loader").spawn(async move {
            match api.get_providers().await {
                Ok(providers) => store.write().unwrap().push(providers),
                Err(e) => error!(error = ?e, "Failed to get proxy providers"),
            }
            loading.store(false, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn provider_health_check(&self, name: String) -> Result<()> {
        info!("Health check for provider: {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        let pending_test = Arc::clone(&self.pending_test);
        pending_test.fetch_add(1, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-provider-health-check").spawn(async move {
            match api.health_check_provider(name).await {
                Ok(_) => _ = action_tx.send(Action::ProxyProviderRefresh),
                Err(e) => error!(error = ?e, "Failed to health check provider"),
            }
            pending_test.fetch_sub(1, Ordering::Relaxed);
        })?;

        Ok(())
    }

    fn update_provider(&self, name: String) -> Result<()> {
        info!("Update provider: {}", name);
        let api = Arc::clone(self.api.as_ref().unwrap());
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        let loading = Arc::clone(&self.loading);
        loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new().name("proxy-provider-update").spawn(async move {
            match api.update_provider(name).await {
                Ok(_) => _ = action_tx.send(Action::ProxyProviderRefresh),
                Err(e) => {
                    error!(error = ?e, "Failed to update provider");
                    let _ = action_tx.send(Action::Error(("Update proxy provider", e).into()));
                }
            }
            loading.store(false, Ordering::Relaxed);
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

    fn render_provider(view: &ProviderView, focused: bool, frame: &mut Frame, area: Rect) {
        let title_line = Line::from(vec![
            Span::styled(view.provider.name.as_str(), Color::White),
            Span::raw(" ("),
            Span::styled(format!("{}", view.provider.proxies.len()), Color::LightCyan),
            Span::raw(") "),
            Span::raw(view.provider.vehicle_type.as_str()),
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
        let inner_width = area.width - 2;

        let mut lines = Vec::with_capacity(4);
        let usage = (inner_width as f64 * view.usage_percent.unwrap_or_default() / 100f64) as usize;
        lines.push(space_between_many(
            inner_width,
            vec![
                Span::styled(bar::THREE_EIGHTHS.repeat(usage), Color::White),
                Span::styled(
                    bar::THREE_EIGHTHS.repeat(inner_width as usize - usage - 6),
                    Color::DarkGray,
                ),
            ],
            Span::styled(format!("{:>6.1}%", view.usage_percent.unwrap_or_default()), Color::Cyan),
        ));
        lines.push(space_between_many(
            inner_width,
            vec![
                Span::styled(
                    view.provider
                        .subscription_info
                        .as_ref()
                        .filter(|v| v.download.is_some() || v.upload.is_some())
                        .map(|v| {
                            human_bytes(
                                (v.download.unwrap_or_default() + v.upload.unwrap_or_default())
                                    as f64,
                                None,
                            )
                        })
                        .unwrap_or("-".to_string()),
                    Color::DarkGray,
                ),
                Span::styled(" / ", Color::DarkGray),
                Span::styled(
                    view.provider
                        .subscription_info
                        .as_ref()
                        .and_then(|v| v.total)
                        .map(|t| human_bytes(t as f64, None))
                        .unwrap_or("-".to_string()),
                    Color::DarkGray,
                ),
            ],
            Span::styled(
                format!(
                    "Expire: {}",
                    view.provider
                        .subscription_info
                        .as_ref()
                        .and_then(|v| v.expire)
                        .and_then(format_timestamp)
                        .unwrap_or("-".to_string())
                ),
                Color::DarkGray,
            ),
        ));
        lines.push(Line::styled(
            format!("Updated at: {}", view.provider.updated_at_str.as_deref().unwrap_or("-")),
            Color::DarkGray,
        ));
        lines.push(view.quality_stats.as_line(inner_width, view.provider.proxies.len()));

        let para = Paragraph::new(lines).block(block);
        frame.render_widget(para, area);
    }

    fn render_providers(&mut self, frame: &mut Frame, outer: Rect) {
        let providers = self.store.read().unwrap().view();

        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw("proxy providers ("),
            Span::styled(format!("{}", providers.len()), Color::LightCyan),
            Span::raw(")"),
            Span::raw(TOP_TITLE_RIGHT),
        ]);
        let block = Block::bordered().border_type(BorderType::Rounded).title(title_line);
        let area = block.inner(outer);
        frame.render_widget(block, outer);

        let col_chunks =
            Layout::horizontal((0..CARDS_PER_ROW).map(|_| Constraint::Fill(1))).split(area);
        self.navigator
            .length(providers.len(), ((area.height / CARD_HEIGHT) as usize) * col_chunks.len());
        self.navigator.iter_visible(&providers, CARD_HEIGHT, col_chunks).for_each(
            |(proxy, focused, rect)| {
                Self::render_provider(proxy, focused, frame, rect);
            },
        );
    }
}

impl Component for ProxyProvidersComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ProxyProviders
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
            Shortcut::from("update", 0).unwrap(),
        ]
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        self.load_providers()?;
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
            KeyCode::Char('r') => {
                if self
                    .loading
                    .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    return Ok(Some(Action::ProxyProviderRefresh));
                }
            }
            KeyCode::Char('s') => return Ok(Some(Action::ProxySetting)),
            KeyCode::Enter => {
                let store = self.store.read().unwrap();
                if let Some(idx) = self.navigator.focused {
                    let action =
                        store.get(idx).map(|v| v.provider.clone()).map(Action::ProxyProviderDetail);
                    return Ok(action);
                }
            }
            KeyCode::Char('t') => {
                let store = self.store.read().unwrap();
                if let Some(idx) = self.navigator.focused
                    && let Some(p) = store.get(idx)
                {
                    self.provider_health_check(p.provider.name.clone())?;
                }
            }
            KeyCode::Char('u') => {
                let store = self.store.read().unwrap();
                if let Some(idx) = self.navigator.focused
                    && let Some(p) = store.get(idx)
                {
                    self.update_provider(p.provider.name.clone())?;
                }
            }
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::ProxyProviderRefresh => self.load_providers()?,
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
        self.render_providers(frame, area);
        self.render_throbber(frame, area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
