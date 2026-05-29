use std::path::PathBuf;
use std::sync::Arc;

use crate::components::ComponentId;
use crate::error::UserError;
use crate::models::Connection;
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
    ConnectionsSetting,
    ConnectionsSettingChanged,
    /// Sent when the filter pattern is changed via user input.
    FilterChanged(Option<String>),
    /// Programmatically sets the filter pattern without re-triggering `FilterChanged`.
    FilterSet(Option<String>),
    ConnectionTerminateRequest(Arc<Connection>),
    ConnectionBatchTerminateRequest(Vec<String>),
    ProxyDetail(String),
    ProxySetting,
    ProxySettingChanged,
    ProxyProviderDetail(String),
}
