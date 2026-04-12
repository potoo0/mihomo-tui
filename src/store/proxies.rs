use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};

use anyhow::Result;
use indexmap::IndexMap;
use tracing::{debug, error, warn};

use crate::api::Api;
use crate::models::proxy::Proxy;
use crate::store::proxy_setting::ProxySetting;
use crate::widgets::latency::{LatencyQuality, QualityStats};

pub static GLOBAL_PROXIES: OnceLock<RwLock<Proxies>> = OnceLock::new();

#[derive(Debug)]
pub struct ProxyView {
    pub proxy: Arc<Proxy>,
    pub quality_stats: QualityStats,
}

#[derive(Debug, Default)]
pub struct Proxies {
    proxies: HashMap<String, Arc<Proxy>>,
    visible: Vec<Arc<ProxyView>>,
}

/// Global store for proxies, providing thread-safe access and update methods.
impl Proxies {
    pub fn global() -> &'static RwLock<Self> {
        GLOBAL_PROXIES.get_or_init(Default::default)
    }

    pub fn get(index: usize) -> Option<Arc<ProxyView>> {
        match Self::global().read() {
            Ok(p) => p.visible.get(index).cloned(),
            Err(e) => {
                error!(error = ?e, "Failed to acquire read lock");
                None
            }
        }
    }

    pub fn get_by_name(name: &str) -> Option<Arc<Proxy>> {
        match Self::global().read() {
            Ok(p) => p.proxies.get(name).cloned(),
            Err(e) => {
                error!(error = ?e, "Failed to acquire read lock");
                None
            }
        }
    }

    pub fn with_by_names<R, F>(names: &[String], f: F) -> R
    where
        F: FnOnce(&[&Arc<Proxy>]) -> R,
    {
        match Self::global().read() {
            Ok(p) => {
                let proxies: Vec<_> = names.iter().flat_map(|name| p.proxies.get(name)).collect();
                f(&proxies)
            }
            Err(e) => {
                error!(error = ?e, "Failed to acquire read lock");
                f(&[])
            }
        }
    }

    pub fn with_view<R, F>(f: F) -> R
    where
        F: FnOnce(&[Arc<ProxyView>]) -> R,
    {
        match Self::global().read() {
            Ok(p) => f(&p.visible),
            Err(e) => {
                error!(error = ?e, "Failed to acquire read lock");
                f(&[])
            }
        }
    }

    /// Load proxies from API and update the store.
    pub async fn load(api: Arc<Api>) -> Result<()> {
        match api.get_proxies().await {
            Ok(proxies) => {
                debug!("Proxies loaded");
                match Self::global().write() {
                    Ok(mut p) => p.push(proxies),
                    Err(e) => error!(error = ?e, "Failed to acquire write lock"),
                }
            }
            Err(e) => return Err(e),
        }

        Ok(())
    }

    /// Update proxy selection and reload proxies.
    pub async fn update_and_reload(api: Arc<Api>, selector: &str, name: &str) -> Result<()> {
        match api.update_proxy(selector, name).await {
            Ok(_) => Self::load(api).await,
            Err(e) => {
                error!(error = ?e, "Failed to update proxy");
                Err(e)
            }
        }
    }

    pub async fn test_and_reload(api: Arc<Api>, name: &str) -> Result<()> {
        let (test_url, test_timeout) = {
            let setting = ProxySetting::global().read().unwrap();
            (setting.test_url.clone(), setting.test_timeout)
        };

        let result = api.test_proxy(name, &test_url, test_timeout).await;
        // Even if testing fails, we still want to
        // reload the proxies to get the latest latency info.
        if let Err(e) = result {
            warn!(error = ?e, "Failed to test proxy: {}", name);
        }
        Self::load(api).await
    }

    pub async fn test_group_and_reload(api: Arc<Api>, name: &str) -> Result<()> {
        let (test_url, test_timeout) = {
            let setting = ProxySetting::global().read().unwrap();
            (setting.test_url.clone(), setting.test_timeout)
        };

        let result = api.test_proxy_group(name, &test_url, test_timeout).await;
        // Even if testing fails, we still want to
        // reload the proxies to get the latest latency info.
        if let Err(e) = result {
            warn!(error = ?e, "Failed to test proxy group: {}", name);
        }
        Self::load(api).await
    }
}

/// Internal methods for managing proxies
impl Proxies {
    pub fn clear(&mut self) {
        self.proxies.clear();
        self.proxies.shrink_to_fit();
        self.visible.clear();
        self.visible.shrink_to_fit();
    }

    pub fn push(&mut self, mut proxies: IndexMap<String, Proxy>) {
        self.remove_missing_children(&mut proxies);
        self.update_delay(&mut proxies);
        self.proxies = proxies.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        let threshold = ProxySetting::global().read().unwrap().threshold;

        let sort_index = self.build_sort_index();
        let mut visible: Vec<Arc<ProxyView>> = self
            .proxies
            .values()
            .filter(|p| !(p.hidden == Some(true) || p.children.as_ref().is_none_or(Vec::is_empty)))
            .map(|v| self.build_proxy_view(v, threshold))
            .collect();
        visible.sort_by_key(|v| sort_index.get(&v.proxy.name).copied().unwrap_or(usize::MAX));

        self.visible = visible;
    }

    fn build_proxy_view(&self, proxy: &Arc<Proxy>, threshold: (u64, u64)) -> Arc<ProxyView> {
        let mut quality_stats = [0; LatencyQuality::COUNT];
        if let Some(ref children) = proxy.children {
            for child in children {
                let quality = self.proxies.get(child).map(|v| v.latency).unwrap_or_default();
                let idx: usize = LatencyQuality::from(quality, threshold).into();
                quality_stats[idx] += 1;
            }
        }

        Arc::new(ProxyView {
            proxy: Arc::clone(proxy),
            quality_stats: QualityStats::new(quality_stats),
        })
    }

    fn remove_missing_children(&self, proxies: &mut IndexMap<String, Proxy>) {
        let keys: HashSet<_> = proxies.keys().cloned().collect();
        for v in proxies.values_mut() {
            if let Some(children) = v.children.as_mut() {
                let missing: Vec<_> = children.iter().filter(|c| !keys.contains(*c)).collect();
                if missing.is_empty() {
                    return;
                }
                warn!("Proxy '{}' has missing children: {:?}", v.name, missing);
                children.retain(|c| keys.contains(c));
            }
        }
    }

    fn update_delay(&self, proxies: &mut IndexMap<String, Proxy>) {
        fn update(key: &str, proxies: &mut IndexMap<String, Proxy>) {
            let (selected, has_children) = {
                let proxy = match proxies.get_mut(key) {
                    // only update if not set
                    Some(p) if p.latency.is_none() => p,
                    _ => return,
                };
                (proxy.selected.clone(), proxy.children.is_some())
            };

            if let (Some(selected), true) = (selected, has_children) {
                // recursively compute delay for selected child
                update(&selected, proxies);
                if let Some(latency) = proxies.get(&selected).map(|p| p.latency)
                    && let Some(proxy) = proxies.get_mut(key)
                {
                    proxy.latency = latency
                }
            } else if let Some(proxy) = proxies.get_mut(key) {
                proxy.latency = proxy.history.last().map(|h| h.delay).into();
            }
        }
        // calculate delay for all proxies
        for k in proxies.keys().cloned().collect::<Vec<_>>() {
            update(&k, proxies);
        }
    }

    fn build_sort_index(&self) -> HashMap<String, usize> {
        self.proxies
            .get("GLOBAL")
            .and_then(|v| v.children.as_ref())
            .into_iter()
            .flat_map(|v| v.iter())
            .enumerate()
            .map(|(idx, key)| (key.clone(), idx))
            .collect()
    }
}
