use std::path::PathBuf;
use std::sync::Arc;

use crate::app_message::AppMessage;
use crate::components::ComponentId;
use crate::models::{Connection, Version};
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
    Info(AppMessage),
    Error(AppMessage),
    AppUpdateRequest,
    SelfUpdate(bool),
    RefreshVersion,
    CoreVersionUpdated(Version),
    /// Spawn an external editor to edit a file. args: `(editor command, file path)`
    SpawnExternalEditor(String, PathBuf),
    Help,
    TabSwitch(ComponentId),
    Shortcuts(Vec<Shortcut>),
    ConnectionDetail(Arc<Connection>),
    ConnectionsSetting(Vec<String>),
    ConnectionsSettingChanged,
    /// Sent when the filter pattern is changed via user input.
    FilterChanged(Option<String>),
    /// Programmatically sets the filter placeholder for the current tab.
    FilterPlaceholder(Option<String>),
    /// Programmatically sets the filter pattern without re-triggering `FilterChanged`.
    FilterSet(Option<String>),
    ConnectionTerminateRequest(Arc<Connection>),
    ConnectionBatchTerminateRequest(Vec<String>),
    ProxyDetail(String),
    ProxySetting,
    ProxySettingChanged,
    ProxyProviderDetail(String),
    DnsQuery,
}
