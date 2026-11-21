use std::str::FromStr;

use strum::IntoStaticStr;

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
    Help,
    Root,
    Header,
    Footer,
    Updates,
    #[default]
    Overview,
    ConnectionDetail,
    ConnectionTerminate,
    ConnectionBatchTerminate,
    Connections,
    ConnectionsSetting,
    Proxies,
    ProxyDetail,
    ProxyInspect,
    ProxySetting,
    ProxyProviders,
    ProxyProviderDetail,
    Logs,
    Rules,
    RuleProviders,
    Config,
    Language,
    DnsQuery,
    Filter,
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
        self.into()
    }
}
