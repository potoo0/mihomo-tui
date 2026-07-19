use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};

use anyhow::Result;
use indexmap::IndexMap;
use tracing::{debug, error, info, warn};

use crate::api::Api;
use crate::config::{LatencyThreshold, ProxySortConfig};
use crate::models::proxy::Proxy;
use crate::models::proxy_provider::ProxyProvider;
use crate::models::sort::{ProxySortField, SortDir};
use crate::store::proxy_setting::ProxySetting;
use crate::widgets::latency::{LatencyQuality, QualityStats};

pub static GLOBAL_PROXIES: OnceLock<RwLock<Proxies>> = OnceLock::new();

/// Special root proxy group used as the source of top-level proxy order.
/// It should not be sorted in proxy-detail group sorting.
const ROOT_PROXY_GROUP: &str = "GLOBAL";

#[derive(Debug)]
pub struct ProxyView {
    pub proxy: Arc<Proxy>,
    pub quality_stats: QualityStats,
}

#[derive(Debug, Default)]
pub struct Proxies {
    sort: Option<ProxySortConfig>,
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
        // `GET /proxies` only contains groups and proxies defined inline in the
        // core config; nodes sourced from proxy providers are only exposed via
        // `GET /providers/proxies`. Merge them in so group children referencing
        // provider nodes can be resolved by name.
        let (proxies, providers) = tokio::join!(api.get_proxies(), api.get_providers());
        let mut proxies = proxies?;
        match providers {
            Ok(providers) => Self::merge_provider_proxies(&mut proxies, providers),
            Err(e) => warn!(error = ?e, "Failed to load providers, provider nodes may be missing"),
        }
        debug!("Proxies loaded");
        match Self::global().write() {
            Ok(mut p) => p.push(proxies),
            Err(e) => error!(error = ?e, "Failed to acquire write lock"),
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
            (setting.test_url.clone(), setting.test_timeout.get())
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
            (setting.test_url.clone(), setting.test_timeout.get())
        };

        let result = api.test_proxy_group(name, &test_url, test_timeout).await;
        // Even if testing fails, we still want to
        // reload the proxies to get the latest latency info.
        if let Err(e) = result {
            warn!(error = ?e, "Failed to test proxy group: {}", name);
        }
        Self::load(api).await
    }

    pub fn init_sort_config(sort: Option<ProxySortConfig>) {
        let mut p = Self::global().write().expect("proxies store poisoned");
        if p.sort.is_none() {
            info!(?sort, "Initializing sort config");
            p.sort = sort;
        }
    }

    fn update_sort_and_reload<F>(api: Arc<Api>, f: F)
    where
        F: FnOnce(Option<ProxySortConfig>) -> Option<ProxySortConfig>,
    {
        {
            let mut p = Self::global().write().expect("proxies store poisoned");
            let old_sort = p.sort.take();
            let new_sort = f(old_sort.clone());
            if old_sort.is_none() && new_sort.is_none() {
                p.sort = new_sort;
                return;
            }
            info!(old = ?old_sort, new = ?new_sort, "Changed proxy detail sort");
            p.sort = new_sort;
        } // release lock

        tokio::task::Builder::new()
            .name("proxies-loader")
            .spawn(async {
                if let Err(e) = Self::load(api).await {
                    error!(error = ?e, "Failed to reload proxies after sort change");
                }
            })
            .expect("Failed to spawn proxies loader task");
    }

    pub fn switch_sort_field(api: Arc<Api>) {
        Self::update_sort_and_reload(api, |old_sort| match old_sort {
            None => Some(ProxySortConfig { field: ProxySortField::Latency, dir: SortDir::Asc }),
            Some(old) => match old.field {
                ProxySortField::Latency => {
                    Some(ProxySortConfig { field: ProxySortField::Name, dir: old.dir })
                }
                ProxySortField::Name => None,
            },
        });
    }

    pub fn toggle_sort_direction(api: Arc<Api>) {
        Self::update_sort_and_reload(api, |old_sort| {
            old_sort.map(|old| ProxySortConfig { dir: old.dir.toggle(), ..old })
        });
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
        Self::remove_missing_children(&mut proxies);
        Self::update_delay(&mut proxies);
        if let Some(sort) = &self.sort {
            Self::sort_proxies(&mut proxies, sort);
        }

        self.proxies = proxies.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        let threshold = ProxySetting::global().read().unwrap().latency_threshold;

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

    fn build_proxy_view(&self, proxy: &Arc<Proxy>, threshold: LatencyThreshold) -> Arc<ProxyView> {
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

    fn build_sort_index(&self) -> HashMap<String, usize> {
        self.proxies
            .get(ROOT_PROXY_GROUP)
            .and_then(|v| v.children.as_ref())
            .into_iter()
            .flat_map(|v| v.iter())
            .enumerate()
            .map(|(idx, key)| (key.clone(), idx))
            .collect()
    }
}

impl Proxies {
    fn sort_proxies(proxies: &mut IndexMap<String, Proxy>, sort_config: &ProxySortConfig) {
        match sort_config.field {
            ProxySortField::Name => Self::sort_proxies_by_name(proxies, sort_config.dir),
            ProxySortField::Latency => Self::sort_proxies_by_latency(proxies, sort_config.dir),
        }
    }

    fn sort_proxies_by_name(proxies: &mut IndexMap<String, Proxy>, dir: SortDir) {
        for proxy in proxies.values_mut() {
            if proxy.name == ROOT_PROXY_GROUP {
                continue;
            }
            let Some(children) = proxy.children.as_mut() else {
                continue;
            };

            children.sort_by(|a, b| match dir {
                SortDir::Asc => a.cmp(b),
                SortDir::Desc => b.cmp(a),
            });
        }
    }

    fn sort_proxies_by_latency(proxies: &mut IndexMap<String, Proxy>, dir: SortDir) {
        let snapshot: HashMap<String, i64> = proxies
            .iter()
            .filter_map(|(key, proxy)| match proxy.latency.0 {
                Some(delay) if delay > 0 => Some((key.clone(), delay)),
                _ => None,
            })
            .collect();

        for proxy in proxies.values_mut() {
            if proxy.name == ROOT_PROXY_GROUP {
                continue;
            }
            let Some(children) = proxy.children.as_mut() else {
                continue;
            };

            children.sort_by(|a, b| {
                let a_latency = snapshot.get(a).copied();
                let b_latency = snapshot.get(b).copied();
                match (a_latency, b_latency) {
                    (Some(a), Some(b)) => match dir {
                        SortDir::Asc => a.cmp(&b),
                        SortDir::Desc => b.cmp(&a),
                    },
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            });
        }
    }

    /// Merge provider-sourced proxies into the map. Entries already present in
    /// `GET /proxies` (groups, inline proxies) take precedence; "Compatible"
    /// providers mirroring groups are therefore effectively ignored.
    fn merge_provider_proxies(
        proxies: &mut IndexMap<String, Proxy>,
        providers: IndexMap<String, ProxyProvider>,
    ) {
        for provider in providers.into_values() {
            for proxy in provider.proxies {
                proxies.entry(proxy.name.clone()).or_insert(proxy);
            }
        }
    }

    fn remove_missing_children(proxies: &mut IndexMap<String, Proxy>) {
        let keys: HashSet<_> = proxies.keys().cloned().collect();
        for v in proxies.values_mut() {
            if let Some(children) = v.children.as_mut() {
                let missing: Vec<_> = children.iter().filter(|c| !keys.contains(*c)).collect();
                if missing.is_empty() {
                    continue;
                }
                warn!("Proxy '{}' has missing children: {:?}", v.name, missing);
                children.retain(|c| keys.contains(c));
            }
        }
    }

    fn update_delay(proxies: &mut IndexMap<String, Proxy>) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProxySortConfig;
    use crate::models::proxy::DelayHistory;
    use crate::models::sort::{ProxySortField, SortDir};

    fn proxy(name: &str, children: Option<Vec<&str>>, latency: Option<i64>) -> Proxy {
        Proxy {
            name: name.to_string(),
            r#type: "Mock".to_string(),
            hidden: None,
            children: children.map(|v| v.into_iter().map(str::to_string).collect()),
            selected: None,
            history: vec![DelayHistory { delay: latency.unwrap_or_default() }],
            latency: latency.into(),
        }
    }

    fn sort_config(field: ProxySortField, dir: SortDir) -> ProxySortConfig {
        ProxySortConfig { field, dir }
    }

    #[test]
    fn test_merge_provider_proxies_adds_nodes_and_keeps_existing() {
        let mut proxies = IndexMap::from([
            ("group".to_string(), proxy("group", Some(vec!["a", "b"]), None)),
            ("a".to_string(), proxy("a", None, Some(10))),
        ]);
        let providers = IndexMap::from([(
            "p1".to_string(),
            ProxyProvider {
                name: "p1".to_string(),
                vehicle_type: "HTTP".to_string(),
                proxies: vec![proxy("a", None, Some(999)), proxy("b", None, Some(20))],
                subscription_info: None,
                updated_at: None,
                updated_at_str: None,
            },
        )]);

        Proxies::merge_provider_proxies(&mut proxies, providers);

        assert_eq!(proxies.len(), 3);
        // existing entry from `/proxies` wins over the provider copy
        assert_eq!(proxies.get("a").unwrap().latency.0, Some(10));
        assert_eq!(proxies.get("b").unwrap().latency.0, Some(20));
    }

    #[test]
    fn test_remove_missing_children_cleans_all_groups() {
        let mut proxies = IndexMap::from([
            ("ok".to_string(), proxy("ok", Some(vec!["a"]), None)),
            ("broken".to_string(), proxy("broken", Some(vec!["a", "ghost"]), None)),
            ("a".to_string(), proxy("a", None, Some(10))),
        ]);

        Proxies::remove_missing_children(&mut proxies);

        // groups after the first clean one must still be processed
        assert_eq!(
            proxies.get("broken").and_then(|p| p.children.clone()).unwrap(),
            vec!["a".to_string()]
        );
    }

    #[test]
    fn test_sort_proxies_by_name_asc() {
        let mut proxies = IndexMap::from([
            ("group".to_string(), proxy("group", Some(vec!["b", "a", "c"]), None)),
            ("a".to_string(), proxy("alpha", None, Some(30))),
            ("b".to_string(), proxy("beta", None, Some(20))),
            ("c".to_string(), proxy("charlie", None, Some(10))),
        ]);

        Proxies::sort_proxies(&mut proxies, &sort_config(ProxySortField::Name, SortDir::Asc));

        assert_eq!(
            proxies.get("group").and_then(|p| p.children.clone()).unwrap(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn test_sort_proxies_by_name_desc() {
        let mut proxies = IndexMap::from([
            ("group".to_string(), proxy("group", Some(vec!["b", "a", "c"]), None)),
            ("a".to_string(), proxy("alpha", None, Some(30))),
            ("b".to_string(), proxy("beta", None, Some(20))),
            ("c".to_string(), proxy("charlie", None, Some(10))),
        ]);

        Proxies::sort_proxies(&mut proxies, &sort_config(ProxySortField::Name, SortDir::Desc));

        assert_eq!(
            proxies.get("group").and_then(|p| p.children.clone()).unwrap(),
            vec!["c".to_string(), "b".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn test_sort_proxies_by_latency_asc() {
        let mut proxies = IndexMap::from([
            ("group".to_string(), proxy("group", Some(vec!["slow", "timeout", "fast"]), None)),
            ("fast".to_string(), proxy("fast", None, Some(10))),
            ("slow".to_string(), proxy("slow", None, Some(50))),
            ("timeout".to_string(), proxy("timeout", None, Some(0))),
        ]);

        Proxies::sort_proxies(&mut proxies, &sort_config(ProxySortField::Latency, SortDir::Asc));

        assert_eq!(
            proxies.get("group").and_then(|p| p.children.clone()).unwrap(),
            vec!["fast".to_string(), "slow".to_string(), "timeout".to_string()]
        );
    }

    #[test]
    fn test_sort_proxies_by_latency_desc() {
        let mut proxies = IndexMap::from([
            ("group".to_string(), proxy("group", Some(vec!["slow", "timeout", "fast"]), None)),
            ("fast".to_string(), proxy("fast", None, Some(10))),
            ("slow".to_string(), proxy("slow", None, Some(50))),
            ("timeout".to_string(), proxy("timeout", None, Some(-1))),
        ]);

        Proxies::sort_proxies(&mut proxies, &sort_config(ProxySortField::Latency, SortDir::Desc));

        assert_eq!(
            proxies.get("group").and_then(|p| p.children.clone()).unwrap(),
            vec!["slow".to_string(), "fast".to_string(), "timeout".to_string()]
        );
    }

    #[test]
    fn test_sort_proxies_by_latency_keeps_stable_order_for_equal_values() {
        let mut proxies = IndexMap::from([
            ("group".to_string(), proxy("group", Some(vec!["second", "first", "timeout"]), None)),
            ("first".to_string(), proxy("alpha", None, Some(20))),
            ("second".to_string(), proxy("beta", None, Some(20))),
            ("timeout".to_string(), proxy("timeout", None, Some(0))),
        ]);

        Proxies::sort_proxies(&mut proxies, &sort_config(ProxySortField::Latency, SortDir::Asc));

        assert_eq!(
            proxies.get("group").and_then(|p| p.children.clone()).unwrap(),
            vec!["second".to_string(), "first".to_string(), "timeout".to_string()]
        );
    }

    #[test]
    fn test_sort_proxies_ignores_proxies_without_children() {
        let mut proxies = IndexMap::from([
            ("group".to_string(), proxy("group", Some(vec!["b", "a"]), None)),
            ("leaf".to_string(), proxy("leaf", None, Some(100))),
            ("a".to_string(), proxy("alpha", None, Some(10))),
            ("b".to_string(), proxy("beta", None, Some(20))),
        ]);

        Proxies::sort_proxies(&mut proxies, &sort_config(ProxySortField::Name, SortDir::Asc));

        assert!(proxies.get("leaf").unwrap().children.is_none());
        assert_eq!(
            proxies.get("group").and_then(|p| p.children.clone()).unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
