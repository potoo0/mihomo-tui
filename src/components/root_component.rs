use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures_util::{StreamExt, TryStreamExt, future};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::connection_batch_terminate_component::ConnectionBatchTerminateComponent;
use crate::components::connection_detail_component::ConnectionDetailComponent;
use crate::components::connection_terminate_component::ConnectionTerminateComponent;
use crate::components::connections_component::ConnectionsComponent;
use crate::components::connections_setting_component::ConnectionsSettingComponent;
use crate::components::core_config_component::CoreConfigComponent;
use crate::components::dns_query_component::DnsQueryComponent;
use crate::components::filter_component::FilterComponent;
use crate::components::footer_component::FooterComponent;
use crate::components::header_component::HeaderComponent;
use crate::components::help_component::HelpComponent;
use crate::components::logs_component::LogsComponent;
use crate::components::msg_box_component::MsgBoxComponent;
use crate::components::overview_component::OverviewComponent;
use crate::components::proxies_component::ProxiesComponent;
use crate::components::proxy_detail_component::ProxyDetailComponent;
use crate::components::proxy_provider_detail_component::ProxyProviderDetailComponent;
use crate::components::proxy_providers_component::ProxyProvidersComponent;
use crate::components::proxy_setting_component::ProxySettingComponent;
use crate::components::rule_providers_component::RuleProvidersComponent;
use crate::components::rules_component::RulesComponent;
use crate::components::updates_component::UpdatesComponent;
use crate::components::{Component, ComponentId, TABS};
use crate::config::Config;
use crate::models::{Connection, ConnectionStats};
use crate::render::RenderRequester;
use crate::utils::text_ui::top_title_line;
use crate::version_update::SharedVersionUpdateState;

/// Minimum terminal area `(width, height)` to render the UI properly.
const MIN_AREA: (u16, u16) = (80, 18);
/// 120 seconds at 4 ticks per second
const IDLE_TICKS: u16 = 120 * 4;

pub struct RootComponent {
    api: Option<Arc<Api>>,
    config: Option<Arc<Config>>,
    action_tx: Option<UnboundedSender<Action>>,
    render_requesters: TabRenderRequesters,
    update_state: SharedVersionUpdateState,

    current_tab: ComponentId,
    idle_tabs: HashMap<ComponentId, u16>,
    components: HashMap<ComponentId, Box<dyn Component>>,

    /// UI priority (input & render): `msg_box` > `focused` > `popup` > `normal`.
    /// Message box lifecycle is owned and eagerly cleared by RootComponent
    msg_box: Option<MsgBoxComponent>,
    focused: Option<ComponentId>,
    popup: Option<ComponentId>,

    conn_token: Option<CancellationToken>,
    stats_tx: watch::Sender<Option<ConnectionStats>>,
    stats_rx: watch::Receiver<Option<ConnectionStats>>,
    conns_tx: mpsc::Sender<Vec<Connection>>,
    conns_rx: Arc<AsyncMutex<mpsc::Receiver<Vec<Connection>>>>,
}

#[derive(Default)]
struct TabRenderRequesters {
    requester: Option<RenderRequester>,
    /// Render notification gates for tabs with continuously running streams.
    gates: HashMap<ComponentId, Arc<AtomicBool>>,
}

impl RootComponent {
    pub fn new() -> Self {
        let update_state = SharedVersionUpdateState::default();
        let components: Vec<Box<dyn Component>> = vec![
            Box::new(HeaderComponent::new(update_state.clone())),
            Box::new(FooterComponent::default()),
        ];
        let components = components.into_iter().map(|c| (c.id(), c)).collect::<HashMap<_, _>>();
        let (stats_tx, stats_rx) = watch::channel(None);
        let (conns_tx, conns_rx) = mpsc::channel(2);

        Self {
            api: Default::default(),
            config: Default::default(),
            current_tab: Default::default(),
            popup: Default::default(),
            focused: Default::default(),
            idle_tabs: Default::default(),
            msg_box: Default::default(),
            components,
            action_tx: Default::default(),
            render_requesters: Default::default(),
            update_state,

            conn_token: Default::default(),
            stats_tx,
            stats_rx,
            conns_tx,
            conns_rx: Arc::new(AsyncMutex::new(conns_rx)),
        }
    }

    fn create_component(
        &self,
        id: ComponentId,
        render_requester: RenderRequester,
    ) -> Result<Box<dyn Component>> {
        let (Some(api), Some(action_tx), Some(config)) = (&self.api, &self.action_tx, &self.config)
        else {
            bail!("component dependencies are not initialized");
        };

        let mut component: Box<dyn Component> = match id {
            ComponentId::Overview => Box::new(OverviewComponent::new(
                self.stats_rx.clone(),
                config.buffer.overview.clone(),
            )),
            ComponentId::Connections => Box::new(ConnectionsComponent::new(
                Arc::clone(&self.conns_rx),
                config.buffer.connections,
            )),
            ComponentId::ConnectionsSetting => Box::new(ConnectionsSettingComponent::default()),
            ComponentId::Proxies => Box::new(ProxiesComponent::default()),
            ComponentId::ProxyDetail => Box::new(ProxyDetailComponent::default()),
            ComponentId::ProxySetting => Box::new(ProxySettingComponent::default()),
            ComponentId::ProxyProviders => Box::new(ProxyProvidersComponent::default()),
            ComponentId::ProxyProviderDetail => Box::new(ProxyProviderDetailComponent::default()),
            ComponentId::Logs => Box::new(LogsComponent::new(config.buffer.logs)),
            ComponentId::Rules => Box::new(RulesComponent::default()),
            ComponentId::RuleProviders => Box::new(RuleProvidersComponent::default()),
            ComponentId::Config => Box::new(CoreConfigComponent::default()),
            ComponentId::Updates => Box::new(UpdatesComponent::new(self.update_state.clone())),
            ComponentId::Help => Box::new(HelpComponent::default()),
            ComponentId::ConnectionDetail => Box::new(ConnectionDetailComponent::default()),
            ComponentId::ConnectionBatchTerminate => {
                Box::new(ConnectionBatchTerminateComponent::default())
            }
            ComponentId::ConnectionTerminate => Box::new(ConnectionTerminateComponent::default()),
            ComponentId::Filter => Box::new(FilterComponent::default()),
            ComponentId::DnsQuery => Box::new(DnsQueryComponent::default()),
            _ => bail!("unsupported component `{:?}`", id),
        };

        debug!("Initializing component `{:?}`", id);
        component.init(Arc::clone(api)).with_context(|| format!("failed to initialize {id:?}"))?;
        component
            .register_action_handler(action_tx.clone())
            .with_context(|| format!("failed to register action handler for {id:?}"))?;
        component
            .register_config_handler(Arc::clone(config))
            .with_context(|| format!("failed to register config handler for {id:?}"))?;
        component
            .register_render_requester(render_requester)
            .with_context(|| format!("failed to register render requester for {id:?}"))?;
        component.start().with_context(|| format!("failed to start {id:?}"))?;

        Ok(component)
    }

    fn get_or_start(&mut self, id: ComponentId) -> Result<&mut Box<dyn Component>> {
        // TODO: Once Polonius is stable, use `get_mut` as an early-return fast path to avoid
        // the second lookup for initialized components (NLL problem case #3).
        if !self.components.contains_key(&id) {
            let render_requester = self.render_requesters.requester_for(id, self.current_tab)?;
            let component = self.create_component(id, render_requester)?;
            self.components.insert(id, component);
        }

        Ok(self.components.get_mut(&id).expect("component must exist after initialization"))
    }

    fn open_popup(&mut self, id: ComponentId) -> Result<()> {
        info!("Opening popup {:?}", id);
        self.popup = Some(id);

        // get and init component, send shortcuts of current tab to footer
        let shortcuts = self.get_or_start(id)?.shortcuts();
        let tx = self.action_tx.as_ref().unwrap();
        tx.send(Action::Shortcuts(shortcuts))?;

        // focus the popup component
        tx.send(Action::Focus(id))?;

        Ok(())
    }

    /// Returns `true` if the connections stream is currently active.
    fn is_conn_active(&self) -> bool {
        self.conn_token.as_ref().is_some_and(|t| !t.is_cancelled())
    }

    /// Returns `true` if the current tab requires the connections stream.
    fn is_conn_tab(&self) -> bool {
        matches!(self.current_tab, ComponentId::Overview | ComponentId::Connections)
    }

    fn should_stop_conn(&self) -> bool {
        !self.is_conn_tab()
            && self.is_conn_active()
            && !self.idle_tabs.contains_key(&ComponentId::Overview)
            && !self.idle_tabs.contains_key(&ComponentId::Connections)
    }

    fn stop_conn(&mut self) {
        if let Some(token) = self.conn_token.take() {
            info!("Stopping connection stream");
            token.cancel();
        }
    }

    /// Start loading connections if needed
    fn maybe_load_conn(&mut self) -> Result<()> {
        if !self.is_conn_tab() || self.is_conn_active() {
            return Ok(());
        }

        let token = CancellationToken::new();
        self.conn_token = Some(token.clone());
        info!("Loading connections");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let stats_tx = self.stats_tx.clone();
        let conns_tx = self.conns_tx.clone();
        let conns_rx = Arc::clone(&self.conns_rx);

        tokio::task::Builder::new().name("connections_wrapper-loader").spawn(async move {
            let stream = match api.stream_connections().await {
                Ok(stream) => stream,
                Err(e) => {
                    error!(error = ?e, "Failed to get connections stream.");
                    return;
                }
            };
            stream
                .take_until(token.cancelled())
                .inspect_err(|e| warn!(error = ?e, "Failed to parse connections."))
                .filter_map(|res| future::ready(res.ok()))
                .for_each(|record| {
                    let _ = stats_tx.send(Some((&record).into()));
                    if let Err(TrySendError::Full(v)) =
                        conns_tx.try_send(record.connections.unwrap_or_default())
                    {
                        // drop oldest
                        if let Ok(mut guard) = conns_rx.try_lock() {
                            let _ = guard.try_recv();
                        }
                        let _ = conns_tx.try_send(v);
                    }
                    future::ready(())
                })
                .await;
        })?;
        Ok(())
    }

    fn area_msg_line<'a>(width: u16, height: u16) -> Line<'a> {
        Line::default().spans(vec![
            "Width = ".bold(),
            Span::raw(width.to_string()).cyan(),
            " Height = ".bold(),
            Span::raw(height.to_string()).cyan(),
        ])
    }

    fn renew_idle(&mut self, to: ComponentId) {
        self.idle_tabs.remove(&to);
        if self.current_tab != to {
            self.idle_tabs.insert(self.current_tab, IDLE_TICKS);
        }
    }

    fn destroy_component(&mut self, id: ComponentId) {
        // double check
        if id == self.current_tab {
            return;
        }
        if self.components.remove(&id).is_some() {
            self.idle_tabs.remove(&id);
            self.render_requesters.remove(id);
            info!("Destroyed idle component {:?}", id);
        }
    }

    fn on_tick(&mut self) {
        // decrement idle counters
        let mut to_remove = vec![];
        for (&id, ticks) in self.idle_tabs.iter_mut() {
            *ticks = ticks.saturating_sub(1);
            if *ticks == 0 {
                to_remove.push(id);
            }
        }
        for id in to_remove {
            self.destroy_component(id);
        }
        // stop connections if no tab needs it
        if self.should_stop_conn() {
            self.stop_conn();
        }
    }

    fn handle_global_shortcut(&mut self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => return Some(Action::Quit),
                KeyCode::Char('l') => {
                    info!("Clearing idle tabs by Ctrl+L shortcut");
                    for id in self.idle_tabs.keys().cloned().collect::<Vec<_>>() {
                        self.destroy_component(id);
                    }
                    return Some(Action::Tick);
                }
                KeyCode::Char('u')
                    if key.modifiers == KeyModifiers::CONTROL
                        && self.popup.is_none()
                        && self.focused.is_none()
                        && self.msg_box.is_none() =>
                {
                    return Some(Action::AppUpdateRequest);
                }
                _ => {}
            }
        }
        None
    }
}

impl TabRenderRequesters {
    fn register(&mut self, requester: RenderRequester) {
        self.requester = Some(requester);
    }

    fn requester_for(
        &mut self,
        id: ComponentId,
        current_tab: ComponentId,
    ) -> Result<RenderRequester> {
        let Some(requester) = self.requester.as_ref() else {
            bail!("render requester is not initialized");
        };
        if !Self::uses_render_gate(id) {
            return Ok(requester.clone());
        }

        let visible = Arc::new(AtomicBool::new(id == current_tab));
        self.gates.insert(id, Arc::clone(&visible));
        Ok(requester.scoped(visible))
    }

    fn uses_render_gate(id: ComponentId) -> bool {
        // Only tabs with continuously running streams need render gates.
        matches!(id, ComponentId::Overview | ComponentId::Connections | ComponentId::Logs)
    }

    fn set_render_gate(&mut self, id: ComponentId, visible: bool) {
        if let Some(gate) = self.gates.get(&id) {
            gate.store(visible, Ordering::Release);
        }
    }

    fn switch_tab(&mut self, from: ComponentId, to: ComponentId) {
        if from != to {
            self.set_render_gate(from, false);
            self.set_render_gate(to, true);
        }
    }

    fn remove(&mut self, id: ComponentId) {
        self.gates.remove(&id);
    }
}

impl Drop for RootComponent {
    fn drop(&mut self) {
        self.stop_conn();
        info!("`RootComponent` dropped, background task cancelled");
    }
}

impl Component for RootComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Root
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(Arc::clone(&api));
        // initialize existing components
        for component in self.components.values_mut() {
            component.init(Arc::clone(&api))?;
        }
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx.clone());
        for component in self.components.values_mut() {
            component.register_action_handler(tx.clone())?;
        }
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> Result<()> {
        self.config = Some(Arc::clone(&config));
        for component in self.components.values_mut() {
            component.register_config_handler(Arc::clone(&config))?;
        }

        Ok(())
    }

    fn register_render_requester(&mut self, requester: RenderRequester) -> Result<()> {
        self.render_requesters.register(requester);
        for (id, component) in self.components.iter_mut() {
            let requester = self.render_requesters.requester_for(*id, self.current_tab)?;
            component.register_render_requester(requester)?;
        }
        Ok(())
    }

    fn start(&mut self) -> Result<()> {
        for component in self.components.values_mut() {
            component.start()?;
        }
        self.maybe_load_conn()
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // handle global shortcuts
        if let Some(action) = self.handle_global_shortcut(key) {
            return Ok(Some(action));
        }

        // The message box component
        if let Some(msg_box) = &self.msg_box {
            if msg_box.should_close_on_key(key) {
                self.msg_box = None;
            }
            return Ok(None);
        }

        // The focused component exclusively handles key events.
        if let Some(focused) = self.focused {
            return self.get_or_start(focused)?.handle_key_event(key);
        }

        match key.code {
            KeyCode::Char('q') => return Ok(Some(Action::Quit)),
            KeyCode::Char('h') => return Ok(Some(Action::Help)),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let index = (c as u8 - b'0') as usize;
                if let Some(component_id) = TABS.get(index.saturating_sub(1)) {
                    self.action_tx.as_ref().unwrap().send(Action::TabSwitch(*component_id))?;
                }
                return Ok(None);
            }
            _ => {}
        }
        debug!("Try handling key event: tab={:?}, key={:?}", self.current_tab, key);
        self.get_or_start(self.current_tab)?.handle_key_event(key)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        match action {
            Action::Quit => self.stop_conn(),
            Action::Tick => self.on_tick(),
            Action::Error(err) => {
                self.msg_box =
                    Some(MsgBoxComponent::error(err.title, err.message, err.msg_box_size));
                return Ok(None);
            }
            Action::Info(info) => {
                self.msg_box =
                    Some(MsgBoxComponent::info(info.title, info.message, info.msg_box_size));
                return Ok(None);
            }
            Action::TabSwitch(to) => {
                let from = self.current_tab;
                self.renew_idle(to);
                self.current_tab = to;
                self.render_requesters.switch_tab(from, to);
                self.maybe_load_conn()?;
                // get and init component, send shortcuts of current tab to footer
                let shortcuts = self.get_or_start(self.current_tab)?.shortcuts();
                if self.current_tab.supports_filter() {
                    self.get_or_start(ComponentId::Filter)?;
                }
                action_tx.send(Action::Shortcuts(shortcuts))?;
            }
            Action::AppUpdateRequest => self.open_popup(ComponentId::Updates)?,
            Action::Help => self.open_popup(ComponentId::Help)?,
            Action::ConnectionDetail(_) => self.open_popup(ComponentId::ConnectionDetail)?,
            Action::ConnectionsSetting(_) => self.open_popup(ComponentId::ConnectionsSetting)?,
            Action::ProxyDetail(_) => self.open_popup(ComponentId::ProxyDetail)?,
            Action::ProxySetting => self.open_popup(ComponentId::ProxySetting)?,
            Action::ProxyProviderDetail(_) => self.open_popup(ComponentId::ProxyProviderDetail)?,
            Action::ConnectionTerminateRequest(_) => {
                self.open_popup(ComponentId::ConnectionTerminate)?
            }
            Action::ConnectionBatchTerminateRequest(_) => {
                self.open_popup(ComponentId::ConnectionBatchTerminate)?
            }
            Action::DnsQuery => self.open_popup(ComponentId::DnsQuery)?,
            Action::Focus(focused) => self.focused = Some(focused),
            Action::Unfocus => {
                self.focused = None;
                // close popup when unfocused
                if self.popup.is_some() {
                    self.popup = None;
                    // send shortcuts of current tab to footer
                    let shortcuts = self.get_or_start(self.current_tab)?.shortcuts();
                    action_tx.send(Action::Shortcuts(shortcuts))?;
                }
            }
            _ => {}
        }
        // propagate action to all non-idle components
        for (component_id, component) in self.components.iter_mut() {
            if self.idle_tabs.contains_key(component_id) {
                continue;
            }

            if let Some(action) = component.update(action.clone())? {
                action_tx.send(action)?;
            }
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if area.width < MIN_AREA.0 || area.height < MIN_AREA.1 {
            let lines = vec![
                Line::from("Terminal size too small:").centered(),
                Self::area_msg_line(area.width, area.height).centered(),
                Line::raw(""),
                Line::from("Expected:").centered(),
                Self::area_msg_line(MIN_AREA.0, MIN_AREA.1).centered(),
            ];
            let block = Block::default()
                .border_type(BorderType::Rounded)
                .title(top_title_line("error", Color::Red))
                .borders(Borders::ALL);
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
            return Ok(());
        }
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);

        // draw header
        self.get_or_start(ComponentId::Header)?.draw(frame, chunks[0])?;

        // draw main area
        if self.current_tab.supports_filter() {
            let inner_chunks =
                Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(chunks[1]);
            self.get_or_start(ComponentId::Filter)?.draw(frame, inner_chunks[0])?;
            self.get_or_start(self.current_tab)?.draw(frame, inner_chunks[1])?;
        } else {
            self.get_or_start(self.current_tab)?.draw(frame, chunks[1])?;
        }

        // draw popup if any
        if let Some(popup) = self.popup {
            self.get_or_start(popup)?.draw(frame, chunks[1])?;
        }
        self.msg_box.as_ref().map(|c| c.draw(frame, area)).transpose()?;

        // draw footer
        // get last row of main area for footer, with margin left/right = 1
        let footer_area = Rect::new(area.x + 1, area.y + area.height - 1, area.width - 2, 1);
        self.get_or_start(ComponentId::Footer)?.draw(frame, footer_area)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;
    use crate::render::RenderScheduler;

    fn has_render_request(scheduler: &RenderScheduler) -> bool {
        scheduler.wait_for_request().now_or_never().is_some()
    }

    #[test]
    fn stream_tab_requester_follows_tab_visibility() {
        let scheduler = RenderScheduler::default();
        let mut requesters = TabRenderRequesters::default();
        requesters.register(scheduler.requester());

        let logs = requesters
            .requester_for(ComponentId::Logs, ComponentId::Rules)
            .expect("render requester is registered");
        logs.request_render();
        assert!(!has_render_request(&scheduler));

        requesters.switch_tab(ComponentId::Rules, ComponentId::Logs);
        logs.request_render();
        assert!(has_render_request(&scheduler));

        requesters.switch_tab(ComponentId::Logs, ComponentId::Rules);
        logs.request_render();
        assert!(!has_render_request(&scheduler));
    }

    #[test]
    fn non_stream_tab_requester_is_not_gated() {
        let scheduler = RenderScheduler::default();
        let mut requesters = TabRenderRequesters::default();
        requesters.register(scheduler.requester());

        let rules = requesters
            .requester_for(ComponentId::Rules, ComponentId::Logs)
            .expect("render requester is registered");
        requesters.switch_tab(ComponentId::Logs, ComponentId::Overview);

        rules.request_render();
        assert!(has_render_request(&scheduler));
    }
}
