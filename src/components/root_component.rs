use crate::action::Action;
use crate::components::footer_component::FooterComponent;
use crate::components::header_component::HeaderComponent;
use crate::components::overview_component::OverviewComponent;
use crate::components::{Component, ComponentId, TABS};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, layout::Rect};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info};

#[derive(Default)]
pub struct RootComponent {
    components: Vec<Box<dyn Component>>,
    action_tx: Option<UnboundedSender<Action>>,
}

impl RootComponent {
    pub fn new() -> Self {
        let components: Vec<Box<dyn Component>> = vec![
            Box::new(HeaderComponent::default()),
            Box::new(FooterComponent::new()),
            Box::new(OverviewComponent::default()),
        ];
        Self {
            components,
            ..Self::default()
        }
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
        match key.code {
            KeyCode::Char('q') => self.action_tx.as_ref().unwrap().send(Action::Quit)?,
            KeyCode::Char('p') => panic!("{}", format!("test panic: {}", ComponentId::Root)),
            KeyCode::Char('e') => error!("test error"),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let index = (c as u8 - b'0') as usize;
                if let Some(component_id) = TABS.get(index.saturating_sub(1)) {
                    self.action_tx.as_ref().unwrap().send(Action::TabSwitch {
                        from: self.components.get(2).unwrap().id(),
                        to: *component_id,
                    })?;
                } else {
                    error!("Invalid tab index: {}", index);
                }
            }
            _ => {
                debug!("Got unexpected key event: {:?}", key);
            }
        }
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        // match action {
        //     _ => (),
        // }
        for component in self.components.iter_mut() {
            component.update(action.clone())?;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        self.components.get_mut(0).unwrap().draw(frame, chunks[0])?;
        self.components.get_mut(2).unwrap().draw(frame, chunks[1])?;
        self.components.get_mut(1).unwrap().draw(frame, chunks[2])?;
        Ok(())
    }
}
