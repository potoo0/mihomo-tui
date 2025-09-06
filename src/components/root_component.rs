use std::collections::HashMap;
use std::sync::Arc;

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures_util::{StreamExt, TryStreamExt, future};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::connection_detail_component::ConnectionDetailComponent;
use crate::components::connections_component::ConnectionsComponent;
use crate::components::footer_component::FooterComponent;
use crate::components::header_component::HeaderComponent;
use crate::components::help_component::HelpComponent;
use crate::components::logs_component::LogsComponent;
use crate::components::overview_component::OverviewComponent;
use crate::components::search_component::SearchComponent;
use crate::components::{AppState, Component, ComponentId, TABS};
use crate::models::{Connection, ConnectionStats};

/// Minimum terminal area `(width, height)` to render the UI properly.
const MIN_AREA: (u16, u16) = (100, 18);

pub struct RootComponent {
    token: CancellationToken,
    api: Option<Arc<Api>>,
    current_tab: ComponentId,
    popup: Option<ComponentId>,
    focused: Option<ComponentId>,
    components: HashMap<ComponentId, Box<dyn Component>>,
    action_tx: Option<UnboundedSender<Action>>,

    stats_tx: watch::Sender<Option<ConnectionStats>>,
    stats_rx: watch::Receiver<Option<ConnectionStats>>,
    conns_tx: broadcast::Sender<Vec<Connection>>,
}

impl RootComponent {
    pub fn new() -> Self {
        let components: Vec<Box<dyn Component>> =
            vec![Box::new(HeaderComponent::default()), Box::new(FooterComponent::default())];
        let components = components.into_iter().map(|c| (c.id(), c)).collect::<HashMap<_, _>>();
        let (stats_tx, stats_rx) = watch::channel::<Option<ConnectionStats>>(None);
        let (conns_tx, _) = broadcast::channel::<Vec<Connection>>(4);

        Self {
            token: CancellationToken::new(),
            api: Default::default(),
            current_tab: Default::default(),
            popup: Default::default(),
            focused: Default::default(),
            components,
            action_tx: Default::default(),

            stats_tx,
            stats_rx,
            conns_tx,
        }
    }

    fn get_or_init(&mut self, id: ComponentId) -> &mut Box<dyn Component> {
        self.components.entry(id).or_insert_with(|| {
            let mut c: Box<dyn Component> = match id {
                ComponentId::Overview => Box::new(OverviewComponent::new(self.stats_rx.clone())),
                ComponentId::Connections => {
                    Box::new(ConnectionsComponent::new(self.conns_tx.subscribe()))
                }
                ComponentId::Logs => Box::new(LogsComponent::default()),
                ComponentId::Help => Box::new(HelpComponent::default()),
                ComponentId::ConnectionDetail => Box::new(ConnectionDetailComponent::default()),
                ComponentId::Search => Box::new(SearchComponent::default()),
                _ => panic!("unsupported component {:?}", id),
            };
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

    fn load_connections(&mut self) -> Result<()> {
        info!("Loading connections");
        let token = self.token.clone();
        let api = Arc::clone(self.api.as_ref().unwrap());
        let stats_tx = self.stats_tx.clone();
        let conns_tx = self.conns_tx.clone();

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
                    let _ = conns_tx.send(record.connections);
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
}

impl Component for RootComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Root
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(Arc::clone(&api));
        self.token = CancellationToken::new();
        // initialize existing components
        for component in self.components.values_mut() {
            component.init(Arc::clone(&api))?;
        }
        self.load_connections()?;
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
        match action {
            Action::Quit => self.token.cancel(),
            Action::TabSwitch(to) => {
                self.current_tab = to;
                // get and init component, send shortcuts of current tab to footer
                let shortcuts = self.get_or_init(self.current_tab).shortcuts();
                self.action_tx.as_ref().unwrap().send(Action::Shortcuts(shortcuts))?;
            }
            Action::Help => self.open_popup(ComponentId::Help)?,
            Action::ConnectionDetail(_) => self.open_popup(ComponentId::ConnectionDetail)?,
            Action::Focus(focused) => self.focused = Some(focused),
            Action::Unfocus => {
                self.focused = None;
                // close popup when unfocused
                if self.popup.is_some() {
                    self.popup = None;
                    // send shortcuts of current tab to footer
                    let shortcuts = self.get_or_init(self.current_tab).shortcuts();
                    self.action_tx.as_ref().unwrap().send(Action::Shortcuts(shortcuts))?;
                }
            }
            _ => {}
        }
        // propagate action to all components
        for component in self.components.values_mut() {
            component.update(action.clone())?;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, state: &AppState) -> Result<()> {
        if area.width < MIN_AREA.0 || area.height < MIN_AREA.1 {
            let lines = vec![
                Line::from("Terminal size too small:").centered(),
                Self::area_msg_line(area.width, area.height).centered(),
                Line::raw(""),
                Line::from("Expected:").centered(),
                Self::area_msg_line(MIN_AREA.0, MIN_AREA.1).centered(),
            ];
            let paragraph = Paragraph::new(lines)
                .block(Block::default().title(Span::raw("Error").red()).borders(Borders::ALL));
            frame.render_widget(paragraph, area);
            return Ok(());
        }
        let chunks =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
                .split(area);

        self.get_or_init(ComponentId::Header).draw(frame, chunks[0], state)?;
        self.get_or_init(ComponentId::Footer).draw(frame, chunks[2], state)?;

        if self.current_tab == ComponentId::Connections || self.current_tab == ComponentId::Logs {
            let inner_chunks =
                Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(chunks[1]);
            self.get_or_init(ComponentId::Search).draw(frame, inner_chunks[0], state)?;
            self.get_or_init(self.current_tab).draw(frame, inner_chunks[1], state)?;
        } else {
            self.get_or_init(self.current_tab).draw(frame, chunks[1], state)?;
        }

        self.popup.map(|c| self.get_or_init(c).draw(frame, chunks[1], state)).transpose()?;

        Ok(())
    }
}
