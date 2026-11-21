mod connections;
mod context;
mod core_config;
mod dns_query;
mod filter;
mod footer;
mod header;
mod help;
mod id;
mod language;
mod logs;
mod msg_box;
mod overview;
mod proxies;
mod proxy_providers;
mod root;
mod rule_providers;
mod rules;
mod updates;

use anyhow::Result;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;

pub use self::context::ComponentContext;
pub use self::id::{ComponentId, TABS};
pub use self::root::RootComponent;
use crate::app::action::Action;
use crate::ui::shortcut::Shortcut;

const HORIZ_STEP: usize = 4;
pub const MIN_CARD_WIDTH: u16 = 18;

/// `Component` is a trait that represents a visual and interactive element of the user interface.
///
/// Implementors of this trait can be registered with the main application loop and will be able to
/// receive events, update state, and be rendered on the screen.
pub trait Component {
    /// Get the unique identifier for the component.
    fn id(&self) -> ComponentId;

    /// Build the action that releases this component's focus.
    #[must_use]
    fn unfocus_action(&self) -> Action {
        Action::Unfocus(self.id())
    }

    /// Get a list of shortcuts associated with the component.
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![]
    }

    /// Start initial loading and background tasks.
    ///
    /// Called once by the application or component factory after construction.
    /// Constructors only assemble dependencies and local state; background work starts here.
    fn start(&mut self) -> Result<()> {
        Ok(())
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
