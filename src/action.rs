use crate::components::ComponentId;
use crate::components::shortcut::Shortcut;

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
}
