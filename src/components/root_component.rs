use std::collections::HashMap;

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;

use crate::action::Action;
use crate::components::connection_detail_component::ConnectionDetailComponent;
use crate::components::connections_component::ConnectionsComponent;
use crate::components::footer_component::FooterComponent;
use crate::components::header_component::HeaderComponent;
use crate::components::logs_component::LogsComponent;
use crate::components::overview_component::OverviewComponent;
use crate::components::{AppState, Component, ComponentId, TABS};

#[derive(Default)]
pub struct RootComponent {
    current_tab: ComponentId,
    components: HashMap<ComponentId, Box<dyn Component>>,
    action_tx: Option<UnboundedSender<Action>>,
    popup: Option<ComponentId>,
}

impl RootComponent {
    pub fn new() -> Self {
        let components: Vec<Box<dyn Component>> = vec![
            Box::new(HeaderComponent::default()),
            Box::new(FooterComponent::default()),
            Box::new(OverviewComponent::default()), // corresponds to ComponentId::Default
        ];
        let components = components
            .into_iter()
            .map(|c| (c.id(), c))
            .collect::<HashMap<_, _>>();
        Self {
            components,
            ..Self::default()
        }
    }

    fn get_or_init(&mut self, id: ComponentId) -> &mut Box<dyn Component> {
        self.components.entry(id).or_insert_with(|| {
            let mut c: Box<dyn Component> = match id {
                ComponentId::Overview => Box::new(OverviewComponent::default()),
                ComponentId::Connections => Box::new(ConnectionsComponent::default()),
                ComponentId::Logs => Box::new(LogsComponent::default()),
                ComponentId::ConnectionDetail => Box::new(ConnectionDetailComponent::default()),
                _ => panic!("unsupported component {:?}", id),
            };
            c.register_action_handler(self.action_tx.as_ref().unwrap().clone())
                .unwrap();
            c
        })
    }
}

impl Component for RootComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Root
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // handle popup first: if it's returned Action::Quit, close the popup; otherwise propagate
        // the action
        if let Some(popup) = self.popup {
            if let Some(action) = self.get_or_init(popup).handle_key_event(key)? {
                if matches!(action, Action::Quit) {
                    self.popup = None;
                    return Ok(None);
                }
                return Ok(Some(action));
            }
            return Ok(None);
        }

        match key.code {
            KeyCode::Char('q') => {
                self.action_tx.as_ref().unwrap().send(Action::Quit)?;
                return Ok(None);
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let index = (c as u8 - b'0') as usize;
                if let Some(component_id) = TABS.get(index.saturating_sub(1)) {
                    self.action_tx
                        .as_ref()
                        .unwrap()
                        .send(Action::TabSwitch(*component_id))?;
                }
                return Ok(None);
            }
            _ => {}
        }
        debug!(
            "Try handling key event: tab={:?}, key={:?}",
            self.current_tab, key
        );
        self.get_or_init(self.current_tab).handle_key_event(key)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::TabSwitch(to) => {
                self.current_tab = to;
                // get and init component, send shortcuts of current tab to footer
                let shortcuts = self.get_or_init(self.current_tab).shortcuts();
                self.action_tx
                    .as_ref()
                    .unwrap()
                    .send(Action::Shortcuts(shortcuts))?;
            }
            Action::ConnectionDetail(_) => {
                self.popup = Some(ComponentId::ConnectionDetail);
                // get and init component, send shortcuts of current tab to footer
                let shortcuts = self.get_or_init(self.popup.unwrap()).shortcuts();
                self.action_tx
                    .as_ref()
                    .unwrap()
                    .send(Action::Shortcuts(shortcuts))?;
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
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        self.get_or_init(ComponentId::Header)
            .draw(frame, chunks[0], state)?;
        self.get_or_init(self.current_tab)
            .draw(frame, chunks[1], state)?;
        self.popup
            .map(|c| self.get_or_init(c).draw(frame, chunks[1], state))
            .transpose()?;
        self.get_or_init(ComponentId::Footer)
            .draw(frame, chunks[2], state)?;
        Ok(())
    }
}
