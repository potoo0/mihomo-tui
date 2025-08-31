use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use color_eyre::Result;
use ratatui::prelude::Rect;
use tokio::select;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{info, trace, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::root_component::RootComponent;
use crate::components::{AppState, Component};
use crate::config::Config;
use crate::models::Version;
use crate::tui::{Event, Tui};

pub struct App {
    config: Config,
    api: Arc<Api>,
    token: CancellationToken,
    state: AppState,
    root: RootComponent,
    should_quit: bool,
    should_suspend: bool,
    live_mode: Arc<AtomicBool>,
    action_tx: UnboundedSender<Action>,
    action_rx: UnboundedReceiver<Action>,
}

impl App {
    pub fn new(config: Config, api: Api) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let state = AppState::default();
        Ok(Self {
            config,
            api: Arc::new(api),
            token: CancellationToken::new(),
            state,
            root: RootComponent::new(),
            should_quit: false,
            should_suspend: false,
            live_mode: Arc::new(AtomicBool::new(true)),
            action_tx,
            action_rx,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        self.state.version = Some(self.load_version().await?);
        self.load_memory().await?;
        self.load_traffic().await?;
        self.load_connections().await?;

        let mut tui = Tui::new()?;
        tui.enter()?;

        self.root.register_action_handler(self.action_tx.clone())?;
        // self.root.register_config_handler(self.config.clone())?;
        self.root.init(tui.size()?)?;

        let action_tx = self.action_tx.clone();
        loop {
            self.handle_events(&mut tui).await?;
            self.handle_actions(&mut tui)?;
            if self.should_suspend {
                tui.suspend()?;
                action_tx.send(Action::Resume)?;
                action_tx.send(Action::ClearScreen)?;
                // tui.mouse(true);
                tui.enter()?;
            } else if self.should_quit {
                tui.stop()?;
                break;
            }
        }
        tui.exit()?;
        Ok(())
    }

    async fn load_version(&mut self) -> Result<Version> {
        info!("Loading version");
        self.api.get_version().await
    }

    async fn load_memory(&mut self) -> Result<()> {
        info!("Loading memory");
        let token = self.token.clone();
        let api = Arc::clone(&self.api);
        let memory_vec = Arc::clone(&self.state.memory);

        tokio::task::Builder::new()
            .name("memory-loader")
            .spawn(async move {
                match api.get_memory().await {
                    Ok(mut stream) => loop {
                        select! {
                            _ = token.cancelled() => {
                                break;
                            },
                            Some(msg) = stream.next() => {
                                match msg {
                                    Ok(memory) if memory.used > 0 => {
                                        let mut guard = memory_vec.lock().unwrap();
                                        guard.push_back(memory);
                                    }
                                    Err(e) => {
                                        warn!("Failed to get memory: {e}");
                                    },
                                    _ => {}
                                }
                            }
                        }
                    },
                    Err(e) => warn!("get memory stream failed: {e}"),
                }
            })?;
        Ok(())
    }

    async fn load_traffic(&mut self) -> Result<()> {
        info!("Loading traffic");
        let token = self.token.clone();
        let api = Arc::clone(&self.api);
        let holder = Arc::clone(&self.state.traffic);

        tokio::task::Builder::new()
            .name("traffic-loader")
            .spawn(async move {
                match api.get_traffic().await {
                    Ok(mut stream) => loop {
                        select! {
                            _ = token.cancelled() => {
                                break;
                            },
                            Some(msg) = stream.next() => {
                                match msg {
                                    Ok(record) => {
                                        let mut guard = holder.lock().unwrap();
                                        guard.push_back(record);
                                    }
                                    Err(e) => {
                                        warn!("Failed to get traffic: {e}");
                                    },
                                }
                            }
                        }
                    },
                    Err(e) => warn!("get traffic stream failed: {e}"),
                }
            })?;
        Ok(())
    }

    async fn load_connections(&mut self) -> Result<()> {
        info!("Loading connections");
        let token = self.token.clone();
        let api = Arc::clone(&self.api);
        let live_mode = Arc::clone(&self.live_mode);
        let conn_stat = Arc::clone(&self.state.conn_stat);
        let connections_vec = Arc::clone(&self.state.connections);

        tokio::task::Builder::new()
            .name("connections-loader")
            .spawn(async move {
                match api.get_connections().await {
                    Ok(mut stream) => loop {
                        select! {
                            _ = token.cancelled() => {
                                break;
                            },
                            Some(msg) = stream.next() => {
                                match msg {
                                    Ok(record) => {
                                        if live_mode.load(Ordering::Relaxed) {
                                        {
                                            let mut stat_guard = conn_stat.lock().unwrap();
                                            *stat_guard = Some((&record).into());
                                        }
                                        {
                                            let mut guard = connections_vec.lock().unwrap();
                                            guard.clear();
                                            guard.extend(record.connections);
                                        }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to get connections: {e}");
                                    },
                                }
                            }
                        }
                    },
                    Err(e) => warn!("get connections stream failed: {e}"),
                }
            })?;
        Ok(())
    }

    async fn handle_events(&mut self, tui: &mut Tui) -> Result<()> {
        let Some(event) = tui.next_event().await else {
            return Ok(());
        };
        trace!("handle_events: {event:?}");
        let action_tx = self.action_tx.clone();
        match event {
            Event::Quit => action_tx.send(Action::Quit)?,
            Event::Tick => action_tx.send(Action::Tick)?,
            Event::Render => action_tx.send(Action::Render)?,
            Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
            _ => {}
        }
        if let Some(action) = self.root.handle_events(Some(event.clone()))? {
            action_tx.send(action)?;
        }
        Ok(())
    }

    fn handle_actions(&mut self, tui: &mut Tui) -> Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            if action != Action::Tick && action != Action::Render {
                trace!("handle_actions: {action:?}");
            }
            match action {
                Action::Tick => {}
                Action::Quit => {
                    self.token.cancel();
                    self.should_quit = true;
                }
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => tui.terminal.clear()?,
                Action::Resize(w, h) => self.handle_resize(tui, w, h)?,
                Action::Render => self.render(tui)?,
                Action::LiveMode(live) => self.live_mode.store(live, Ordering::Relaxed),
                Action::RequestConnectionDetail(index) => {
                    let guard = self.state.connections.lock().unwrap();
                    if let Some(conn) = guard.get(index) {
                        self.action_tx
                            .send(Action::ConnectionDetail(conn.clone()))?;
                    }
                }
                _ => {}
            }
            if let Some(action) = self.root.update(action.clone())? {
                self.action_tx.send(action)?
            };
        }
        Ok(())
    }

    fn handle_resize(&mut self, tui: &mut Tui, w: u16, h: u16) -> Result<()> {
        tui.resize(Rect::new(0, 0, w, h))?;
        self.render(tui)?;
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        tui.draw(|frame| {
            if let Err(err) = self.root.draw(frame, frame.area(), &self.state) {
                let _ = self
                    .action_tx
                    .send(Action::Error(format!("Failed to draw: {:?}", err)));
            }
        })?;
        Ok(())
    }
}
