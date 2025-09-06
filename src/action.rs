use crate::components::ComponentId;
use crate::components::shortcut::Shortcut;
use crate::models::Connection;
use crate::models::search_query::OrderBy;

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
    /// request connection detail by index in the connections
    RequestConnectionDetail(usize),
    /// render connection detail
    ConnectionDetail(Box<Connection>),
    LiveMode(bool),
    SearchInputChanged(Option<String>),
    Ordering(Option<OrderBy>),
}
