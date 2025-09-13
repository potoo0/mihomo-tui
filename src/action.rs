use std::sync::Arc;

use crate::components::ComponentId;
use crate::components::shortcut::Shortcut;
use crate::models::Connection;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    #[allow(dead_code)]
    Suspend,
    Resume,
    Quit,
    Focus(ComponentId),
    Unfocus,
    ClearScreen,
    Error(String),
    Help,
    TabSwitch(ComponentId),
    Shortcuts(Vec<Shortcut>),
    ConnectionDetail(Arc<Connection>),
    SearchInputChanged(Option<String>),
    ConnectionTerminateRequest(Arc<Connection>),
}
