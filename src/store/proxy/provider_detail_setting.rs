use std::ops::{Deref, DerefMut};

use super::ProxyDetailSetting;
use crate::config::ProxyDetailUiConfig;
use crate::store::hub::StoreLifecycle;

#[derive(Clone, Debug, Default)]
pub struct ProxyProviderDetailSetting(pub ProxyDetailSetting);

impl Deref for ProxyProviderDetailSetting {
    type Target = ProxyDetailSetting;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ProxyProviderDetailSetting {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl StoreLifecycle for ProxyProviderDetailSetting {}

impl From<&ProxyDetailUiConfig> for ProxyProviderDetailSetting {
    fn from(value: &ProxyDetailUiConfig) -> Self {
        Self(value.into())
    }
}

impl From<ProxyProviderDetailSetting> for ProxyDetailUiConfig {
    fn from(value: ProxyProviderDetailSetting) -> Self {
        value.0.into()
    }
}
