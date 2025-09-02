use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use color_eyre::Result;
use futures_util::{StreamExt, TryStreamExt, future};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::prelude::Rect;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::columns::CONNECTION_COLS;
use crate::components::root_component::RootComponent;
use crate::components::{AppState, Component};
use crate::config::Config;
use crate::models::{Connection, Version};
use crate::tui::{Event, Tui};

pub struct App {
    _config: Config,
    api: Arc<Api>,
    token: CancellationToken,
    state: AppState,
    matcher: Arc<SkimMatcherV2>,
    root: RootComponent,

    should_quit: bool,
    should_suspend: bool,
    action_tx: UnboundedSender<Action>,
    action_rx: UnboundedReceiver<Action>,
}

impl App {
    pub fn new(_config: Config, api: Api) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let state = AppState::new(true);
        let matcher = SkimMatcherV2::default();
        Ok(Self {
            _config,
            api: Arc::new(api),
            token: CancellationToken::new(),
            state,
            matcher: Arc::new(matcher),
            root: RootComponent::new(),

            should_quit: false,
            should_suspend: false,
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
        let store = Arc::clone(&self.state.memory);

        tokio::task::Builder::new()
            .name("memory-loader")
            .spawn(async move {
                let stream = match api.get_memory().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        warn!("Failed to get memory stream: {e}");
                        return;
                    }
                };
                stream
                    .take_until(token.cancelled())
                    .inspect_err(|e| warn!("Failed to parse memory: {e}"))
                    .filter_map(|res| future::ready(res.ok()))
                    .for_each(|record| {
                        if record.used > 0 {
                            store.lock().unwrap().push_back(record);
                        }
                        future::ready(())
                    })
                    .await;
            })?;
        Ok(())
    }

    async fn load_traffic(&mut self) -> Result<()> {
        info!("Loading traffic");
        let token = self.token.clone();
        let api = Arc::clone(&self.api);
        let store = Arc::clone(&self.state.traffic);

        tokio::task::Builder::new()
            .name("traffic-loader")
            .spawn(async move {
                let stream = match api.get_traffic().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        warn!("Failed to get traffic stream: {e}");
                        return;
                    }
                };
                stream
                    .take_until(token.cancelled())
                    .inspect_err(|e| warn!("Failed to parse traffic: {e}"))
                    .filter_map(|res| future::ready(res.ok()))
                    .for_each(|record| {
                        store.lock().unwrap().push_back(record);
                        future::ready(())
                    })
                    .await;
            })?;
        Ok(())
    }

    async fn load_connections(&mut self) -> Result<()> {
        info!("Loading connections");
        let token = self.token.clone();
        let api = Arc::clone(&self.api);
        let live_mode = Arc::clone(&self.state.live_mode);
        let conn_stat = Arc::clone(&self.state.conn_stat);
        let connections_vec = Arc::clone(&self.state.connections);
        let matcher = Arc::clone(&self.matcher);
        let filter_pattern = Arc::clone(&self.state.filter_pattern);
        let ordering = Arc::clone(&self.state.ordering);

        tokio::task::Builder::new()
            .name("connections-loader")
            .spawn(async move {
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
                        if live_mode.load(Ordering::Relaxed) {
                            *conn_stat.lock().unwrap() = Some((&record).into());
                            {
                                let pat = filter_pattern.read().unwrap().clone();
                                let pat = pat.as_deref();
                                let connections =
                                    Self::filter_connections(&matcher, pat, record.connections);
                                let ord = *ordering.read().unwrap();
                                let connections = if let Some((col, desc)) = ord {
                                    let mut conns = connections;
                                    let col_def = &CONNECTION_COLS[col];
                                    conns.sort_by(|a, b| col_def.ordering(a, b, desc));
                                    conns
                                } else {
                                    connections
                                };

                                let mut guard = connections_vec.lock().unwrap();
                                guard.clear();
                                guard.extend(connections);
                            }
                        }
                        future::ready(())
                    })
                    .await;
            })?;
        Ok(())
    }

    fn filter_connections(
        matcher: &SkimMatcherV2,
        pattern: Option<&str>,
        src: Vec<Connection>,
    ) -> Vec<Connection> {
        let pat = match pattern {
            Some(p) if !p.is_empty() => p,
            _ => return src,
        };

        src.into_iter()
            .filter(|c| {
                CONNECTION_COLS
                    .iter()
                    .filter(|col| col.filterable)
                    .any(|col| {
                        let text: Cow<'_, str> = (col.accessor)(c);
                        matcher.fuzzy_match(&text, pat).is_some()
                    })
            })
            .collect()
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
                Action::LiveMode(live) => self.state.live_mode.store(live, Ordering::Relaxed),
                Action::TabSwitch(to) => {
                    self.state.focused = to;
                    *self.state.filter_pattern.write().unwrap() = None;
                    *self.state.ordering.write().unwrap() = None;
                }
                Action::RequestConnectionDetail(index) => {
                    let guard = self.state.connections.lock().unwrap();
                    if let Some(conn) = guard.get(index) {
                        self.action_tx
                            .send(Action::ConnectionDetail(Box::new(conn.clone())))?;
                    }
                }
                Action::SearchInputChanged(ref pattern) => {
                    debug!("Search changed: {:?} at {:?}", pattern, self.state.focused);
                    *self.state.filter_pattern.write().unwrap() = pattern.clone();
                }
                Action::Ordering(ref ord) => {
                    debug!("Ordering changed: {:?} at {:?}", ord, self.state.focused);
                    *self.state.ordering.write().unwrap() = *ord;
                    if let Some((col, desc)) = *ord
                        && !self.state.live_mode.load(Ordering::Relaxed)
                    {
                        let mut guard = self.state.connections.lock().unwrap();
                        let mut conns: Vec<Connection> = guard.drain(..).collect();
                        let col_def = &CONNECTION_COLS[col];
                        conns.sort_by(|a, b| col_def.ordering(a, b, desc));
                        guard.extend(conns);
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_fuzzy_match() {
        use fuzzy_matcher::FuzzyMatcher;
        use fuzzy_matcher::skim::SkimMatcherV2;

        let matcher = SkimMatcherV2::default();
        let text = "nginx: worker process";

        let pattern = "nginx";
        let score = matcher.fuzzy_match(text, pattern);
        assert!(score.is_some());
        println!("Score: {:?}", score);

        let pattern = "wrk";
        let score = matcher.fuzzy_match(text, pattern);
        assert!(score.is_some());
        println!("Score: {:?}", score);

        let pattern = "apache";
        let score = matcher.fuzzy_match(text, pattern);
        assert!(score.is_none());
        println!("Score: {:?}", score);

        let pattern = "krw";
        let score = matcher.fuzzy_match(text, pattern);
        assert!(score.is_none());
        println!("Score: {:?}", score);
    }
}
