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
pub use self::id::{ComponentId, FocusTarget, PopupId, TABS};
pub use self::root::RootComponent;
use crate::app::action::Action;
use crate::ui::shortcut::Shortcut;

const HORIZ_STEP: usize = 4;
pub const MIN_CARD_WIDTH: u16 = 18;

/// Identity shared by components and their erased trait objects.
pub trait ComponentIdentity {
    fn id(&self) -> ComponentId;
}

/// `Component` is a trait that represents a visual and interactive element of the user interface.
///
/// Implementors of this trait can be registered with the main application loop and will be able to
/// receive events, update state, and be rendered on the screen.
pub trait Component: ComponentIdentity {
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

/// A popup supplies its typed identity and can request its own closure.
pub trait PopupComponent: Component {
    fn popup_id(&self) -> PopupId;

    /// Request closure and destruction of this popup, preserving lower layers.
    #[must_use]
    fn close_action(&self) -> Action {
        self.popup_id().close_action()
    }
}

// A popup defines its identity only through PopupId. The blanket implementation
// prevents a second, inconsistent ComponentId implementation for that type.
impl<T: PopupComponent> ComponentIdentity for T {
    fn id(&self) -> ComponentId {
        ComponentId::Popup(self.popup_id())
    }
}
