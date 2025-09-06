use std::collections::HashMap;

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;

use crate::action::Action;
use crate::components::connection_detail_component::ConnectionDetailComponent;
use crate::components::connections_component::ConnectionsComponent;
use crate::components::footer_component::FooterComponent;
use crate::components::header_component::HeaderComponent;
use crate::components::help_component::HelpComponent;
use crate::components::logs_component::LogsComponent;
use crate::components::overview_component::OverviewComponent;
use crate::components::search_component::SearchComponent;
use crate::components::{AppState, Component, ComponentId, TABS};

#[derive(Default)]
pub struct RootComponent {
    current_tab: ComponentId,
    components: HashMap<ComponentId, Box<dyn Component>>,
    action_tx: Option<UnboundedSender<Action>>,
    popup: Option<ComponentId>,
    focused: Option<ComponentId>,
}

impl RootComponent {
    pub fn new() -> Self {
        let components: Vec<Box<dyn Component>> = vec![
            Box::new(HeaderComponent::default()),
            Box::new(FooterComponent::default()),
            Box::new(OverviewComponent::default()), // corresponds to ComponentId::Default
        ];
        let components = components.into_iter().map(|c| (c.id(), c)).collect::<HashMap<_, _>>();
        Self { components, ..Self::default() }
    }

    fn get_or_init(&mut self, id: ComponentId) -> &mut Box<dyn Component> {
        self.components.entry(id).or_insert_with(|| {
            let mut c: Box<dyn Component> = match id {
                ComponentId::Overview => Box::new(OverviewComponent::default()),
                ComponentId::Connections => Box::new(ConnectionsComponent::default()),
                ComponentId::Logs => Box::new(LogsComponent::default()),
                ComponentId::Help => Box::new(HelpComponent::default()),
                ComponentId::ConnectionDetail => Box::new(ConnectionDetailComponent::default()),
                ComponentId::Search => Box::new(SearchComponent::default()),
                _ => panic!("unsupported component {:?}", id),
            };
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
