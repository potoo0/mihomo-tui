use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use ratatui::layout::Rect;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace};

use crate::api::Api;
use crate::app::action::Action;
use crate::components::{Component, ComponentContext, RootComponent};
use crate::config::Config;
use crate::store::hub::StoreHub;
use crate::ui::terminal::{Event, Tui};

pub mod action;
mod external_editor;
pub mod message;
mod render_scheduler;
mod runtime_config_saver;
mod self_update;

pub(crate) use render_scheduler::RenderScheduler;
use runtime_config_saver::RuntimeConfigSaver;

const MAX_ACTIONS_PER_BATCH: usize = 1024;

pub struct App {
    config: Arc<Config>,
    token: CancellationToken,
    root: RootComponent,

    action_tx: UnboundedSender<Action>,
    action_rx: UnboundedReceiver<Action>,
    render_scheduler: RenderScheduler,
    runtime_config_saver: RuntimeConfigSaver,
}

impl App {
    pub fn new(config: Config, runtime_path: PathBuf, api: Api) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let startup_tab = config.startup_tab();
        let render_scheduler = RenderScheduler::default();
        render_scheduler.requester().request_render();
        let config = Arc::new(config);
        let api = Arc::new(api);
        let stores = StoreHub::default();
        let ctx = ComponentContext {
            config: Arc::clone(&config),
            api: Arc::clone(&api),
            stores: stores.clone(),
            actions: action_tx.clone(),
            render: render_scheduler.requester(),
        };
        let runtime_config_saver =
            RuntimeConfigSaver::new(config.runtime_config, runtime_path, stores, action_tx.clone());
        Ok(Self {
            config,
            token: CancellationToken::new(),
            root: RootComponent::new(&ctx, startup_tab),

            action_tx,
            action_rx,
            render_scheduler,
            runtime_config_saver,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut tui = Tui::new()?;
        tui.enter()?;

        // initialize root component
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
            // Drain all queued changes before the final save on quit.
            while processed < MAX_ACTIONS_PER_BATCH || should_quit {
                let Ok(action) = self.action_rx.try_recv() else { break };
                should_quit |= self.handle_action(&mut tui, action)?;
                processed += 1;
            }

            self.runtime_config_saver.finish_batch(should_quit)?;
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
            Action::LanguageChanged(language) => {
                crate::i18n::set_language(*language);
            }
            Action::SelfUpdate(restart) => self.handle_self_update(tui, *restart)?,
            _ => {}
        }

        self.runtime_config_saver.on_action(&action);
        if let Some(action) = self.root.update(action)? {
            self.action_tx.send(action)?
        };

        Ok(should_quit)
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
