use std::sync::{OnceLock, RwLock};

#[derive(Clone, Copy, Default)]
pub struct ProxyDetailSetting {
    pub card_width: Option<u16>,
}

static GLOBAL_PROXY_DETAIL_SETTING: OnceLock<RwLock<ProxyDetailSetting>> = OnceLock::new();

impl ProxyDetailSetting {
    pub fn global() -> &'static RwLock<Self> {
        GLOBAL_PROXY_DETAIL_SETTING.get_or_init(Default::default)
    }

    pub fn snapshot() -> Self {
        *Self::global().read().unwrap()
    }

    pub fn update(f: impl FnOnce(&mut Self)) {
        f(&mut Self::global().write().unwrap());
    }
}
