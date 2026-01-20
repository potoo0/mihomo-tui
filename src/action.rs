use std::path::PathBuf;
use std::sync::Arc;

use crate::components::ComponentId;
use crate::error::UserError;
use crate::models::Connection;
use crate::models::proxy::Proxy;
use crate::models::proxy_provider::ProxyProvider;
use crate::widgets::shortcut::Shortcut;

#[derive(Debug, Clone)]
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
    Error(UserError),
    /// Spawn an external editor to edit a file. args: `(editor command, file path)`
    SpawnExternalEditor(String, PathBuf),
    Help,
    TabSwitch(ComponentId),
    Shortcuts(Vec<Shortcut>),
    ConnectionDetail(Arc<Connection>),
    /// Search -> target components: search query changed by user input
    SearchInputChanged(Option<String>),
    /// Target components -> Search: programmatically set the search input
    /// (does not emit `SearchInputChanged`).
    SearchInputSet(Option<String>),
    ConnectionTerminateRequest(Arc<Connection>),
    ProxyDetail(Arc<Proxy>, Vec<Arc<Proxy>>),
    ProxyUpdateRequest(String, String),
    ProxyDetailRefresh(usize),
    ProxiesRefresh,
    ProxySetting,
    ProxyTestRequest(String),
    ProxyGroupTestRequest(String),
    ProxyProviderDetail(Arc<ProxyProvider>),
    ProxyProviderRefresh,
}
