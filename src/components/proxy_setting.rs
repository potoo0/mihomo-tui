use std::sync::{OnceLock, RwLock};

pub static GLOBAL_PROXY_SETTING: OnceLock<RwLock<ProxySetting>> = OnceLock::new();

#[derive(Debug)]
pub struct ProxySetting {
    pub test_url: String,
    pub test_timeout: u64,
    pub threshold: (u64, u64),
}

impl Default for ProxySetting {
    fn default() -> Self {
        Self {
            test_url: "https://www.gstatic.com/generate_204".into(),
            test_timeout: 5000,
            threshold: (500, 1000),
        }
    }
}

pub fn get_proxy_setting() -> &'static RwLock<ProxySetting> {
    GLOBAL_PROXY_SETTING.get_or_init(Default::default)
}
