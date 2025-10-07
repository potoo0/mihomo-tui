mod connection_detail_component;
mod connection_terminate_component;
mod connections;
mod connections_component;
mod footer_component;
mod header_component;
mod help_component;
mod logs;
mod logs_component;
mod overview_component;
pub mod proxies;
mod proxies_component;
mod proxy_detail_component;
mod proxy_setting;
mod proxy_setting_component;
pub mod root_component;
mod search_component;
pub mod shortcut;
pub mod state;

use std::sync::Arc;

use color_eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use strum::Display;
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::api::Api;
use crate::components::shortcut::Shortcut;
use crate::tui::Event;

const TABS: [ComponentId; 4] =
    [ComponentId::Overview, ComponentId::Connections, ComponentId::Proxies, ComponentId::Logs];
const BUFFER_SIZE: usize = 100;
const CONNS_BUFFER_SIZE: usize = 500;
const LOGS_BUFFER_SIZE: usize = 500;

#[derive(Default, PartialEq, Debug, Display, Clone, Eq, Hash, Copy)]
pub enum ComponentId {
    Help,
    Root,
    Header,
    Footer,
    #[default]
    Overview,
    ConnectionDetail,
    ConnectionTerminate,
    Connections,
    Proxies,
    ProxyDetail,
    ProxySetting,
    Logs,
    Search,
}

/// `Component` is a trait that represents a visual and interactive element of the user interface.
///
/// Implementors of this trait can be registered with the main application loop and will be able to
/// receive events, update state, and be rendered on the screen.
pub trait Component {
    /// Get the unique identifier for the component.
    fn id(&self) -> ComponentId;

    /// Get a list of shortcuts associated with the component.
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![]
    }

    /// Initialize the component with a specified area if necessary.
    ///
    /// # Arguments
    ///
    /// * `api` - An mihomo API instance.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - An Ok result or an error.
    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        let _ = api; // to appease clippy
        Ok(())
    }

    /// Register an action handler that can send actions for processing if necessary.
    ///
    /// # Arguments
    ///
    /// * `tx` - An unbounded sender that can send actions.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - An Ok result or an error.
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        let _ = tx; // to appease clippy
        Ok(())
    }

    /// Handle incoming events and produce actions if necessary.
    ///
    /// # Arguments
    ///
    /// * `event` - An optional event to be processed.
    ///
    /// # Returns
    ///
    /// * `Result<Option<Action>>` - An action to be processed or none.
    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        let action = match event {
            Some(Event::Key(key_event)) => self.handle_key_event(key_event)?,
            Some(Event::Mouse(mouse_event)) => self.handle_mouse_event(mouse_event)?,
            _ => None,
        };
        Ok(action)
    }

    /// Handle key events and produce actions if necessary.
    ///
    /// # Arguments
    ///
    /// * `key` - A key event to be processed.
    ///
    /// # Returns
    ///
    /// * `Result<Option<Action>>` - An action to be processed or none.
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        let _ = key; // to appease clippy
        Ok(None)
    }

    /// Handle mouse events and produce actions if necessary.
    ///
    /// # Arguments
    ///
    /// * `mouse` - A mouse event to be processed.
    ///
    /// # Returns
    ///
    /// * `Result<Option<Action>>` - An action to be processed or none.
    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<Option<Action>> {
        let _ = mouse; // to appease clippy
        Ok(None)
    }

    /// Update the state of the component based on a received action. (REQUIRED)
    ///
    /// # Arguments
    ///
    /// * `action` - An action that may modify the state of the component.
    ///
    /// # Returns
    ///
    /// * `Result<Option<Action>>` - An action to be processed or none.
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        let _ = action; // to appease clippy
        Ok(None)
    }

    /// Render the component on the screen. (REQUIRED)
    ///
    /// # Arguments
    ///
    /// * `f` - A frame used for rendering.
    /// * `area` - The area in which the component should be drawn.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - An Ok result or an error.
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;
}
