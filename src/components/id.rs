use std::str::FromStr;

use strum::IntoStaticStr;

use crate::app::action::Action;

/// Header tabs in display order; index is used for tab navigation and shortcuts
pub const TABS: [ComponentId; 8] = [
    ComponentId::Overview,
    ComponentId::Connections,
    ComponentId::Proxies,
    ComponentId::ProxyProviders,
    ComponentId::Logs,
    ComponentId::Rules,
    ComponentId::RuleProviders,
    ComponentId::Config,
];

#[derive(Default, PartialEq, Debug, IntoStaticStr, Clone, Eq, Hash, Copy)]
pub enum ComponentId {
    Root,
    Header,
    Footer,
    #[default]
    Overview,
    Connections,
    Proxies,
    ProxyProviders,
    Logs,
    Rules,
    RuleProviders,
    Config,
    Filter,
    Popup(PopupId),
}

impl FromStr for ComponentId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        TABS.iter()
            .copied()
            .find(|id| {
                id.full_name().eq_ignore_ascii_case(value)
                    || id.short_name().is_some_and(|name| name.eq_ignore_ascii_case(value))
            })
            .ok_or(())
    }
}

impl ComponentId {
    pub fn supports_filter(self) -> bool {
        matches!(
            self,
            ComponentId::Connections
                | ComponentId::Logs
                | ComponentId::Rules
                | ComponentId::RuleProviders
        )
    }

    pub fn short_name(self) -> Option<&'static str> {
        match self {
            ComponentId::Overview => Some("View"),
            ComponentId::Connections => Some("Conn"),
            ComponentId::Proxies => Some("Pxy"),
            ComponentId::ProxyProviders => Some("Pxy-Pr"),
            ComponentId::Logs => Some("Log"),
            ComponentId::Rules => Some("Rule"),
            ComponentId::RuleProviders => Some("R-Pr"),
            ComponentId::Config => Some("Cfg"),
            _ => Some(self.full_name()),
        }
    }

    pub fn full_name(self) -> &'static str {
        match self {
            Self::Popup(id) => id.into(),
            _ => self.into(),
        }
    }
}

/// Popup identities are separate from tabs and persistent components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum PopupId {
    Help,
    Updates,
    Language,
    ConnectionDetail,
    ConnectionTerminate,
    ConnectionBatchTerminate,
    ConnectionsSetting,
    ProxyDetail,
    ProxyInspect,
    ProxySetting,
    ProxyProviderDetail,
    DnsQuery,
}

impl PopupId {
    /// Request closure and destruction, rather than temporary loss of focus.
    pub fn close_action(self) -> Action {
        Action::Unfocus(FocusTarget::Popup(self))
    }
}

impl From<PopupId> for ComponentId {
    fn from(id: PopupId) -> Self {
        Self::Popup(id)
    }
}

/// Only the filter and popup layers can receive exclusive input focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Filter,
    Popup(PopupId),
}

impl From<FocusTarget> for ComponentId {
    fn from(target: FocusTarget) -> Self {
        match target {
            FocusTarget::Filter => Self::Filter,
            FocusTarget::Popup(id) => Self::Popup(id),
        }
    }
}
