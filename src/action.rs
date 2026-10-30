use std::path::PathBuf;
use std::sync::Arc;

use crate::app_message::AppMessage;
use crate::components::ComponentId;
use crate::models::{Connection, Version};
use crate::widgets::shortcut::Shortcut;

#[derive(Debug, Clone)]
pub enum Action {
    Tick,
    LanguageSelect,
    LanguageChanged(crate::i18n::Language),
    Resize(u16, u16),
    Quit,
    Focus(ComponentId),
    Unfocus,
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
    /// Sent when connection layout settings change without affecting the data view.
    ConnectionsLayoutChanged,
    /// Sent when the filter pattern is changed via user input.
    FilterChanged(Option<String>),
    /// Programmatically sets the filter placeholder field list for the current tab.
    FilterPlaceholder(Option<Vec<&'static str>>),
    /// Programmatically sets the filter pattern without re-triggering `FilterChanged`.
    FilterSet(Option<String>),
    ConnectionTerminateRequest(Arc<Connection>),
    ConnectionBatchTerminateRequest(Vec<String>),
    ProxyDetail(String),
    ProxyDetailLayoutChanged,
    ProxySetting,
    ProxySettingChanged,
    ProxyProviderDetail(String),
    ProxyProviderDetailLayoutChanged,
    DnsQuery,
    DnsQueryResultReady,
}

impl Action {
    /// Whether handlers for this action must run even for idle tabs.
    ///
    /// - `Quit` is a shutdown signal for the component's lifetime, so idle tabs must still finalize
    ///   pending work before the app exits.
    /// - `LanguageChanged` updates locale-dependent state used by cached filter views, so idle tabs
    ///   must recompute those views.
    pub fn broadcasts_to_idle(&self) -> bool {
        matches!(self, Self::Quit | Self::LanguageChanged(_))
    }
}
