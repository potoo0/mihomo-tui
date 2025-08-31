#[warn(dead_code)]
use crate::components::ComponentId;
use crate::components::shortcut::Shortcut;
use crate::models::Connection;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    Error(String),
    Help,
    TabSwitch(ComponentId),
    Shortcuts(Vec<Shortcut>),
    /// request connection detail by index in the connections
    RequestConnectionDetail(usize),
    /// render connection detail
    ConnectionDetail(Connection),
    LiveMode(bool),
}
