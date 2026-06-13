use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::{env, thread};

use anyhow::{Context, Result, anyhow};
use ratatui::layout::Rect;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, trace};

use crate::action::Action;
use crate::api::Api;
use crate::app_message::AppMessage;
use crate::components::root_component::RootComponent;
use crate::components::{Component, ComponentId};
use crate::config::{Config, runtime};
use crate::store::connections_setting::ConnectionsSetting;
use crate::store::proxy_setting::ProxySetting;
use crate::tui::{Event, Tui};
use crate::version_update;
use crate::version_update::RestartOutcome;

pub struct App {
    config: Arc<Config>,
    runtime_path: PathBuf,
    api: Arc<Api>,
    token: CancellationToken,
    root: RootComponent,

    should_quit: bool,
    should_suspend: bool,
    action_tx: UnboundedSender<Action>,
    action_rx: UnboundedReceiver<Action>,
}

impl App {
    pub fn new(config: Config, runtime_path: PathBuf, api: Api) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        Ok(Self {
            config: Arc::new(config),
            runtime_path,
            api: Arc::new(api),
            token: CancellationToken::new(),
            root: RootComponent::new(),

            should_quit: false,
            should_suspend: false,
            action_tx,
            action_rx,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut tui = Tui::new()?;
        tui.enter()?;

        // initialize global settings
        *ProxySetting::global().write().unwrap() = self.config.proxy_setting.clone();
        if let Some(connections) = self.config.ui.as_ref().and_then(|ui| ui.connections.as_ref()) {
            *ConnectionsSetting::global().write().unwrap() = Arc::new(connections.try_into()?);
        }
        // initialize root component
        self.root.init(Arc::clone(&self.api))?;
        self.root.register_action_handler(self.action_tx.clone())?;
        self.root.register_config_handler(Arc::clone(&self.config))?;

        let action_tx = self.action_tx.clone();
        // send initial tab
        action_tx.send(Action::TabSwitch(ComponentId::default()))?;
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
                Action::SpawnExternalEditor(ref editor, ref filepath) => {
                    self.handle_spawn_external_editor(tui, editor, filepath)?
                }
                Action::ConnectionsSettingChanged | Action::ProxySettingChanged => {
                    if let Err(e) = self.save_runtime_config() {
                        error!(error = ?e, "Failed to save runtime config");
                        self.action_tx.send(Action::Error(
                            AppMessage::from(("Save runtime config", e)).msg_box_size(60, 30),
                        ))?;
                    }
                }
                Action::SelfUpdate(restart) => self.handle_self_update(tui, restart)?,
                _ => {}
            }
            if let Some(action) = self.root.update(action.clone())? {
                self.action_tx.send(action)?
            };
        }
        Ok(())
    }

    fn save_runtime_config(&self) -> Result<()> {
        let connections = ConnectionsSetting::snapshot();
        let proxy_setting = ProxySetting::global().read().unwrap().clone();
        runtime::save(&self.runtime_path, &connections, &proxy_setting)
    }

    fn handle_self_update(&mut self, tui: &mut Tui, restart: bool) -> Result<()> {
        let exe_path = env::current_exe().context("get current exe path")?;
        tui.exit()?;

        let action = match thread::spawn(version_update::update_app)
            .join()
            .map_err(|_| anyhow!("app self update thread panicked"))?
        {
            Ok(self_update::Status::UpToDate(version)) => Action::Info(
                AppMessage::from(("Update app", format!("app is already up to date ({version}).")))
                    .msg_box_size(45, 30),
            ),
            Ok(self_update::Status::Updated(version)) if restart => {
                info!(version, "app updated, trying to restart...");
                println!("app updated to {version}. Trying to restart...");
                match version_update::restart_app(&exe_path)? {
                    RestartOutcome::Restarted => return Ok(()),
                    RestartOutcome::Unsupported => Action::Info(
                        AppMessage::from((
                            "Update app",
                            format!(
                                "app updated to {version}. \
                                 Auto restart is not supported on Windows. \
                                 Please restart to use the new version."
                            ),
                        ))
                        .msg_box_size(45, 30),
                    ),
                }
            }
            Ok(self_update::Status::Updated(version)) => {
                println!("app updated to {version}. Please restart to use the new version.");
                Action::Info(
                    AppMessage::from((
                        "Update app",
                        format!("app updated to {version}. Please restart to use the new version."),
                    ))
                    .msg_box_size(45, 30),
                )
            }
            Err(e) => {
                error!(error = ?e, "app self update failed");
                Action::Error(("Update app", e).into())
            }
        };

        tui.enter()?;
        tui.terminal.clear()?;
        self.action_tx.send(action)?;
        self.action_tx.send(Action::RefreshVersion)?;

        Ok(())
    }

    fn handle_spawn_external_editor(
        &self,
        tui: &mut Tui,
        editor: &str,
        filepath: &PathBuf,
    ) -> Result<()> {
        tui.exit()?;

        info!("Spawning external editor `{}` for file `{:?}`...", editor, filepath);
        // print to stdout, so that user can see it in terminal
        println!("Spawning external editor `{}` for file `{:?}`...", editor, filepath);
        match Command::new(editor).arg(filepath).status() {
            Ok(status) => {
                if !status.success() {
                    error!(
                        editor = editor,
                        status_code = ?status.code(),
                        "Editor exited with non-zero status"
                    );
                    let msg =
                        format!("Editor `{}` exited with non-zero status: {}", editor, status);
                    self.action_tx.send(Action::Error(("Spawning external editor", msg).into()))?;
                }
            }
            Err(e) => {
                error!("Failed to spawn editor `{}`: {}", editor, e);
                self.action_tx.send(Action::Error(("Spawning external editor", e).into()))?;
            }
        }

        tui.enter()?;
        tui.terminal.clear()?;

        Ok(())
    }

    fn handle_resize(&mut self, tui: &mut Tui, w: u16, h: u16) -> Result<()> {
        tui.resize(Rect::new(0, 0, w, h))?;
        self.render(tui)?;
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        tui.draw(|frame| {
            if let Err(err) = self.root.draw(frame, frame.area()) {
                error!(error = ?err, "Failed to draw ROOT component");
            }
        })?;
        Ok(())
    }
}
