use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::{env, thread};

use anyhow::{Context, Result, anyhow};
use ratatui::layout::Rect;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace};

use crate::action::Action;
use crate::api::Api;
use crate::app_message::AppMessage;
use crate::components::Component;
use crate::components::root_component::RootComponent;
use crate::config::{Config, runtime};
use crate::render::RenderScheduler;
use crate::store::connections_setting::ConnectionsSetting;
use crate::store::proxy_detail_setting::ProxyDetailSetting;
use crate::store::proxy_provider_detail_setting::ProxyProviderDetailSetting;
use crate::store::proxy_setting::ProxySetting;
use crate::tui::{Event, Tui};
use crate::version_update;
use crate::version_update::RestartOutcome;

const MAX_ACTIONS_PER_BATCH: usize = 1024;

pub struct App {
    config: Arc<Config>,
    runtime_path: PathBuf,
    api: Arc<Api>,
    token: CancellationToken,
    root: RootComponent,

    action_tx: UnboundedSender<Action>,
    action_rx: UnboundedReceiver<Action>,
    render_scheduler: RenderScheduler,
}

impl App {
    pub fn new(config: Config, runtime_path: PathBuf, api: Api) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let startup_tab = config.startup_tab();
        let render_scheduler = RenderScheduler::default();
        render_scheduler.requester().request_render();
        Ok(Self {
            config: Arc::new(config),
            runtime_path,
            api: Arc::new(api),
            token: CancellationToken::new(),
            root: RootComponent::new(startup_tab),

            action_tx,
            action_rx,
            render_scheduler,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut tui = Tui::new()?;
        tui.enter()?;

        self.init_global_settings()?;
        // initialize root component
        self.root.init(Arc::clone(&self.api))?;
        self.root.register_action_handler(self.action_tx.clone())?;
        self.root.register_config_handler(Arc::clone(&self.config))?;
        self.root.register_render_requester(self.render_scheduler.requester())?;
        self.root.start()?;

        // send initial tab
        self.action_tx.send(Action::TabSwitch(self.config.startup_tab()))?;
        loop {
            let deadline = self.render_scheduler.deadline();
            let first_action = tokio::select! {
                event = tui.next_event() => self.handle_event(event.unwrap_or(Event::Quit))?,
                action = self.action_rx.recv() => Some(action.unwrap_or(Action::Quit)),
                _ = self.render_scheduler.wait_for_request() => None,
                _ = RenderScheduler::wait(deadline) => None,
            };
            self.render_scheduler.request();

            let mut should_quit = match first_action {
                Some(action) => self.handle_action(&mut tui, action)?,
                None => false,
            };

            let mut processed = 1;
            // Keep the existing quit flush: Quit can enqueue layout-save actions.
            while processed < MAX_ACTIONS_PER_BATCH || should_quit {
                let Ok(action) = self.action_rx.try_recv() else { break };
                should_quit |= self.handle_action(&mut tui, action)?;
                processed += 1;
            }

            if should_quit {
                break;
            }
            self.render_scheduler.consume_pending_request();
            // Check after every bounded batch, even when select keeps receiving
            // ready events/actions, so they cannot starve a due frame.
            if self.render_scheduler.is_due() {
                self.render(&mut tui)?;
                self.render_scheduler.rendered();
            }
        }
        tui.exit()?;
        Ok(())
    }

    fn init_global_settings(&self) -> Result<()> {
        let ui = self.config.ui.as_ref();
        *ProxySetting::global().write().unwrap() = self.config.proxy_setting.clone();
        ProxyDetailSetting::update(|setting| {
            setting.card_width =
                ui.and_then(|ui| ui.proxy_detail.as_ref()).and_then(|detail| detail.card_width);
        });
        ProxyProviderDetailSetting::update(|setting| {
            setting.card_width = ui
                .and_then(|ui| ui.proxy_provider_detail.as_ref())
                .and_then(|detail| detail.card_width);
        });
        if let Some(connections) = ui.and_then(|ui| ui.connections.as_ref()) {
            *ConnectionsSetting::global().write().unwrap() = Arc::new(connections.try_into()?);
        }
        Ok(())
    }

    fn handle_event(&mut self, event: Event) -> Result<Option<Action>> {
        trace!("handle_event: {event:?}");
        let action = match event {
            Event::Quit => Some(Action::Quit),
            Event::Tick => Some(Action::Tick),
            Event::Resize(w, h) => Some(Action::Resize(w, h)),
            Event::Key(key) => self.root.handle_key_event(key)?,
            Event::Mouse(mouse) => self.root.handle_mouse_event(mouse)?,
            _ => None,
        };
        Ok(action)
    }

    fn handle_action(&mut self, tui: &mut Tui, action: Action) -> Result<bool> {
        let should_quit = matches!(&action, Action::Quit);
        match &action {
            Action::Tick => {}
            Action::Quit => self.token.cancel(),
            Action::Resize(w, h) => self.handle_resize(tui, *w, *h)?,
            // Repaint page transitions to remove terminal artifacts outside the diff buffer.
            Action::TabSwitch(_) => tui.clear_screen()?,
            Action::SpawnExternalEditor(editor, filepath) => {
                self.handle_spawn_external_editor(tui, editor, filepath)?
            }
            Action::LanguageChanged(_)
            | Action::ConnectionsSettingChanged
            | Action::ConnectionsLayoutChanged
            | Action::ProxyDetailLayoutChanged
            | Action::ProxyProviderDetailLayoutChanged
            | Action::ProxySettingChanged => {
                if let Action::LanguageChanged(language) = action {
                    crate::i18n::set_language(language);
                }
                if let Err(e) = self.save_runtime_config() {
                    error!(error = ?e, "Failed to save runtime config");
                    self.action_tx.send(Action::Error(
                        AppMessage::from(("Save runtime config", e)).msg_box_size(60, 30),
                    ))?;
                }
            }
            Action::SelfUpdate(restart) => self.handle_self_update(tui, *restart)?,
            _ => {}
        }

        if let Some(action) = self.root.update(action)? {
            self.action_tx.send(action)?
        };

        Ok(should_quit)
    }

    fn save_runtime_config(&self) -> Result<()> {
        if !self.config.runtime_config {
            return Ok(());
        }
        let connections = ConnectionsSetting::snapshot();
        let proxy_setting = ProxySetting::global().read().unwrap().clone();
        runtime::save(
            &self.runtime_path,
            &connections,
            &proxy_setting,
            ProxyDetailSetting::snapshot(),
            ProxyProviderDetailSetting::snapshot(),
        )
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
        tui.clear_screen()?;
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
        tui.clear_screen()?;

        Ok(())
    }

    fn handle_resize(&mut self, tui: &mut Tui, w: u16, h: u16) -> Result<()> {
        debug!("Resizing to {}x{}", w, h);
        tui.resize(Rect::new(0, 0, w, h))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_runtime_config_does_not_create_or_modify_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.runtime.yaml");
        let mut config = crate::config::default_config().unwrap();
        config.runtime_config = false;
        let api = Api::new(&config).unwrap();
        let app = App::new(config, path.clone(), api).unwrap();

        app.save_runtime_config().unwrap();
        assert!(!path.exists());

        let existing = "existing runtime settings";
        std::fs::write(&path, existing).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        app.save_runtime_config().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), existing);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), modified);

        // An invalid destination would fail if saving were attempted.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        app.save_runtime_config().unwrap();
    }
}
