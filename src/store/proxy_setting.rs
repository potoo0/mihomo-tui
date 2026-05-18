use std::sync::{OnceLock, RwLock};

pub use crate::config::ProxySetting;

pub static GLOBAL_PROXY_SETTING: OnceLock<RwLock<ProxySetting>> = OnceLock::new();

impl ProxySetting {
    pub fn global() -> &'static RwLock<Self> {
        GLOBAL_PROXY_SETTING.get_or_init(Default::default)
    }
}
