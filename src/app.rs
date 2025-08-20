use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::Rect;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::components::{Component, root_component::RootComponent};
use crate::{
    action::Action,
    config::Config,
    tui::{Event, Tui},
};

pub struct App {
    config: Config,
    root: RootComponent,
    // todo cleanup
    // components: Vec<Box<dyn Component>>,
    should_quit: bool,
    should_suspend: bool,
    mode: Mode,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Home,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        Ok(Self {
            config,
            root: RootComponent::new(),
            should_quit: false,
            should_suspend: false,
            mode: Mode::Home,
            last_tick_key_events: Vec::new(),
            action_tx,
            action_rx,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut tui = Tui::new()?;
        tui.enter()?;

        self.root.register_action_handler(self.action_tx.clone())?;
        self.root.register_config_handler(self.config.clone())?;
        self.root.init(tui.size()?)?;
        // for component in self.components.iter_mut() {
        //     component.register_action_handler(self.action_tx.clone())?;
        // }
        // for component in self.components.iter_mut() {
        //     component.register_config_handler(self.config.clone())?;
        // }
        // for component in self.components.iter_mut() {
        //     component.init(tui.size()?)?;
        // }

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

    async fn handle_events(&mut self, tui: &mut Tui) -> Result<()> {
        let Some(event) = tui.next_event().await else {
            return Ok(());
        };
        let action_tx = self.action_tx.clone();
        match event {
            Event::Quit => action_tx.send(Action::Quit)?,
            Event::Tick => action_tx.send(Action::Tick)?,
            Event::Render => action_tx.send(Action::Render)?,
            Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
            Event::Key(key) => self.handle_key_event(key)?,
            _ => {}
        }
        if let Some(action) = self.root.handle_events(Some(event.clone()))? {
            action_tx.send(action)?;
        }
        // for component in self.components.iter_mut() {
        //     if let Some(action) = component.handle_events(Some(event.clone()))? {
        //         action_tx.send(action)?;
        //     }
        // }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        let action_tx = self.action_tx.clone();
        match key.code {
            KeyCode::Char('q') => action_tx.send(Action::Quit)?,
            KeyCode::Char('p') => panic!("{}", format!("test panic: {:?}", &self.mode)),
            KeyCode::Char('e') => error!("test error"),
            _ => {
                info!("Got unexpected key event: {:?}", key);
            }
        }
        // todo
        // let Some(keymap) = self.config.keybindings.get(&self.mode) else {
        //     return Ok(());
        // };
        // match keymap.get(&vec![key]) {
        //     Some(action) => {
        //         info!("Got action: {action:?}");
        //         action_tx.send(action.clone())?;
        //     }
        //     _ => {
        //         // If the key was not handled as a single key action,
        //         // then consider it for multi-key combinations.
        //         self.last_tick_key_events.push(key);
        //
        //         // Check for multi-key combinations
        //         if let Some(action) = keymap.get(&self.last_tick_key_events) {
        //             info!("Got action: {action:?}");
        //             action_tx.send(action.clone())?;
        //         }
        //     }
        // }
        Ok(())
    }

    fn handle_actions(&mut self, tui: &mut Tui) -> Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            if action != Action::Tick && action != Action::Render {
                debug!("{action:?}");
            }
            match action {
                Action::Tick => {
                    self.last_tick_key_events.drain(..);
                }
                Action::Quit => self.should_quit = true,
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => tui.terminal.clear()?,
                Action::Resize(w, h) => self.handle_resize(tui, w, h)?,
                Action::Render => self.render(tui)?,
                _ => {}
            }
            if let Some(action) = self.root.update(action.clone())? {
                self.action_tx.send(action)?
            };
            // for component in self.components.iter_mut() {
            //     if let Some(action) = component.update(action.clone())? {
            //         self.action_tx.send(action)?
            //     };
            // }
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
            if let Err(err) = self.root.draw(frame, frame.area()) {
                let _ = self
                    .action_tx
                    .send(Action::Error(format!("Failed to draw: {:?}", err)));
            }
            // for component in self.components.iter_mut() {
            //     if let Err(err) = component.draw(frame, frame.area()) {
            //         let _ = self
            //             .action_tx
            //             .send(Action::Error(format!("Failed to draw: {:?}", err)));
            //     }
            // }
        })?;
        Ok(())
    }
}
