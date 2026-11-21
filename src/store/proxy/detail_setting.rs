use crate::config::{ProxyDetailUiConfig, ProxySortConfig};
use crate::store::hub::StoreLifecycle;

#[derive(Clone, Debug, Default)]
pub struct ProxyDetailSetting {
    pub sort: Option<ProxySortConfig>,
    pub card_width: Option<u16>,
}

impl StoreLifecycle for ProxyDetailSetting {}

impl From<&ProxyDetailUiConfig> for ProxyDetailSetting {
    fn from(value: &ProxyDetailUiConfig) -> Self {
        Self { sort: value.sort.clone(), card_width: value.card_width }
    }
}

impl From<ProxyDetailSetting> for ProxyDetailUiConfig {
    fn from(value: ProxyDetailSetting) -> Self {
        Self { sort: value.sort, card_width: value.card_width }
    }
}
