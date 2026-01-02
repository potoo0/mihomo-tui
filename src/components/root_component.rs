use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
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
use tracing::{debug, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::connection_detail_component::ConnectionDetailComponent;
use crate::components::connection_terminate_component::ConnectionTerminateComponent;
use crate::components::connections_component::ConnectionsComponent;
use crate::components::footer_component::FooterComponent;
use crate::components::header_component::HeaderComponent;
use crate::components::help_component::HelpComponent;
use crate::components::logs_component::LogsComponent;
use crate::components::overview_component::OverviewComponent;
use crate::components::proxies_component::ProxiesComponent;
use crate::components::proxy_detail_component::ProxyDetailComponent;
use crate::components::proxy_provider_detail_component::ProxyProviderDetailComponent;
use crate::components::proxy_providers_component::ProxyProvidersComponent;
use crate::components::proxy_setting_component::ProxySettingComponent;
use crate::components::search_component::SearchComponent;
use crate::components::{Component, ComponentId, TABS};
use crate::models::{Connection, ConnectionStats};
use crate::utils::text_ui::top_title_line;

/// Minimum terminal area `(width, height)` to render the UI properly.
const MIN_AREA: (u16, u16) = (100, 18);
/// 120 seconds at 4 ticks per second
const IDLE_TICKS: u16 = 120 * 4;

pub struct RootComponent {
    api: Option<Arc<Api>>,
    current_tab: ComponentId,
    popup: Option<ComponentId>,
    focused: Option<ComponentId>,
    idle_tabs: HashMap<ComponentId, u16>,
    components: HashMap<ComponentId, Box<dyn Component>>,
    action_tx: Option<UnboundedSender<Action>>,

    conn_token: Option<CancellationToken>,
    stats_tx: watch::Sender<Option<ConnectionStats>>,
    stats_rx: watch::Receiver<Option<ConnectionStats>>,
    conns_tx: mpsc::Sender<Vec<Connection>>,
    conns_rx: Arc<AsyncMutex<mpsc::Receiver<Vec<Connection>>>>,
}

impl RootComponent {
    pub fn new() -> Self {
        let components: Vec<Box<dyn Component>> =
            vec![Box::new(HeaderComponent::default()), Box::new(FooterComponent::default())];
        let components = components.into_iter().map(|c| (c.id(), c)).collect::<HashMap<_, _>>();
        let (stats_tx, stats_rx) = watch::channel(None);
        let (conns_tx, conns_rx) = mpsc::channel(2);

        Self {
            api: Default::default(),
            current_tab: Default::default(),
            popup: Default::default(),
            focused: Default::default(),
            idle_tabs: Default::default(),
            components,
            action_tx: Default::default(),

            conn_token: Default::default(),
            stats_tx,
            stats_rx,
            conns_tx,
            conns_rx: Arc::new(AsyncMutex::new(conns_rx)),
        }
    }

    fn get_or_init(&mut self, id: ComponentId) -> &mut Box<dyn Component> {
        self.components.entry(id).or_insert_with(|| {
            let mut c: Box<dyn Component> = match id {
                ComponentId::Overview => Box::new(OverviewComponent::new(self.stats_rx.clone())),
                ComponentId::Connections => {
                    Box::new(ConnectionsComponent::new(Arc::clone(&self.conns_rx)))
                }
                ComponentId::Proxies => Box::new(ProxiesComponent::default()),
                ComponentId::ProxyDetail => Box::new(ProxyDetailComponent::default()),
                ComponentId::ProxySetting => Box::new(ProxySettingComponent::default()),
                ComponentId::ProxyProviders => Box::new(ProxyProvidersComponent::default()),
                ComponentId::ProxyProviderDetail => {
                    Box::new(ProxyProviderDetailComponent::default())
                }
                ComponentId::Logs => Box::new(LogsComponent::new()),
                ComponentId::Help => Box::new(HelpComponent::default()),
                ComponentId::ConnectionDetail => Box::new(ConnectionDetailComponent::default()),
                ComponentId::ConnectionTerminate => {
                    Box::new(ConnectionTerminateComponent::default())
                }
                ComponentId::Search => Box::new(SearchComponent::default()),
                _ => panic!("unsupported component `{:?}`", id),
            };
            debug!("Initializing component `{:?}`", id);
            c.init(Arc::clone(self.api.as_ref().unwrap())).unwrap();
            c.register_action_handler(self.action_tx.as_ref().unwrap().clone()).unwrap();
            c
        })
    }

    fn open_popup(&mut self, id: ComponentId) -> Result<()> {
        self.popup = Some(id);

        // get and init component, send shortcuts of current tab to footer
        let shortcuts = self.get_or_init(id).shortcuts();
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
            let stream = match api.get_connections().await {
                Ok(stream) => stream,
                Err(e) => {
                    warn!("Failed to get connections stream: {e}");
                    return;
                }
            };
            stream
                .take_until(token.cancelled())
                .inspect_err(|e| warn!("Failed to parse connections: {e}"))
                .filter_map(|res| future::ready(res.ok()))
                .for_each(|record| {
                    let _ = stats_tx.send(Some((&record).into()));
                    if let Err(TrySendError::Full(v)) = conns_tx.try_send(record.connections) {
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
        self.maybe_load_conn()?;
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // The focused component exclusively handles key events.
        if let Some(focused) = self.focused {
            return self.get_or_init(focused).handle_key_event(key);
        }

        match key.code {
            KeyCode::Char('q') => return Ok(Some(Action::Quit)),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('h') => {
                return Ok(Some(Action::Help));
            }
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
        self.get_or_init(self.current_tab).handle_key_event(key)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        let action_tx = self.action_tx.as_ref().unwrap().clone();
        match action {
            Action::Quit => self.stop_conn(),
            Action::Tick => self.on_tick(),
            Action::TabSwitch(to) => {
                self.renew_idle(to);
                self.current_tab = to;
                self.maybe_load_conn()?;
                // get and init component, send shortcuts of current tab to footer
                let shortcuts = self.get_or_init(self.current_tab).shortcuts();
                action_tx.send(Action::Shortcuts(shortcuts))?;
            }
            Action::Help => self.open_popup(ComponentId::Help)?,
            Action::ConnectionDetail(_) => self.open_popup(ComponentId::ConnectionDetail)?,
            Action::ProxyDetail(_, _) => self.open_popup(ComponentId::ProxyDetail)?,
            Action::ProxySetting => self.open_popup(ComponentId::ProxySetting)?,
            Action::ProxyProviderDetail(_) => self.open_popup(ComponentId::ProxyProviderDetail)?,
            Action::ConnectionTerminateRequest(_) => {
                self.open_popup(ComponentId::ConnectionTerminate)?
            }
            Action::Focus(focused) => self.focused = Some(focused),
            Action::Unfocus => {
                self.focused = None;
                // close popup when unfocused
                if self.popup.is_some() {
                    self.popup = None;
                    // send shortcuts of current tab to footer
                    let shortcuts = self.get_or_init(self.current_tab).shortcuts();
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
        self.get_or_init(ComponentId::Header).draw(frame, chunks[0])?;

        // draw main area
        if self.current_tab == ComponentId::Connections || self.current_tab == ComponentId::Logs {
            let inner_chunks =
                Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(chunks[1]);
            self.get_or_init(ComponentId::Search).draw(frame, inner_chunks[0])?;
            self.get_or_init(self.current_tab).draw(frame, inner_chunks[1])?;
        } else {
            self.get_or_init(self.current_tab).draw(frame, chunks[1])?;
        }

        // draw popup if any
        self.popup.map(|c| self.get_or_init(c).draw(frame, chunks[1])).transpose()?;

        // draw footer
        // get last row of main area for footer, with margin left/right = 1
        let footer_area = Rect::new(area.x + 1, area.y + area.height - 1, area.width - 2, 1);
        self.get_or_init(ComponentId::Footer).draw(frame, footer_area)?;
        Ok(())
    }
}
