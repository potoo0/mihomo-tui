use std::sync::{OnceLock, RwLock};

#[derive(Clone, Copy, Default)]
pub struct ProxyProviderDetailSetting {
    pub card_width: Option<u16>,
}

static GLOBAL_PROXY_PROVIDER_DETAIL_SETTING: OnceLock<RwLock<ProxyProviderDetailSetting>> =
    OnceLock::new();

impl ProxyProviderDetailSetting {
    pub fn global() -> &'static RwLock<Self> {
        GLOBAL_PROXY_PROVIDER_DETAIL_SETTING.get_or_init(Default::default)
    }

    pub fn snapshot() -> Self {
        *Self::global().read().unwrap()
    }

    pub fn update(f: impl FnOnce(&mut Self)) {
        f(&mut Self::global().write().unwrap());
    }
}
