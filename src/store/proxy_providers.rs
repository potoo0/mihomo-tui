use std::cmp::Ordering;
use std::sync::{Arc, OnceLock, RwLock};

use anyhow::Result;
use indexmap::IndexMap;
use tracing::{error, info};

use crate::api::Api;
use crate::config::ProxySortConfig;
use crate::models::proxy_provider::ProxyProvider;
use crate::models::sort::{ProxySortField, SortDir};
use crate::store::proxy_setting::ProxySetting;
use crate::utils::time::format_datetime;
use crate::widgets::latency::{LatencyQuality, QualityStats};

pub static GLOBAL_PROXY_PROVIDERS: OnceLock<RwLock<ProxyProviders>> = OnceLock::new();

#[derive(Debug)]
pub struct ProviderView {
    pub provider: Arc<ProxyProvider>,
    pub quality_stats: QualityStats,
    pub usage_percent: Option<f64>,
}

#[derive(Debug, Default)]
pub struct ProxyProviders {
    sort: Option<ProxySortConfig>,
    providers: Vec<Arc<ProviderView>>,
}

/// Global store for providers, providing thread-safe access and update methods.
impl ProxyProviders {
    pub fn global() -> &'static RwLock<Self> {
        GLOBAL_PROXY_PROVIDERS.get_or_init(Default::default)
    }

    pub fn get(index: usize) -> Option<Arc<ProviderView>> {
        match Self::global().read() {
            Ok(p) => p.providers.get(index).cloned(),
            Err(e) => {
                error!(error = ?e, "Failed to acquire read lock");
                None
            }
        }
    }

    pub fn get_by_name(name: &str) -> Option<(usize, Arc<ProviderView>)> {
        match Self::global().read() {
            Ok(p) => p
                .providers
                .iter()
                .enumerate()
                .find(|(_, v)| v.provider.name == name)
                .map(|(idx, v)| (idx, Arc::clone(v))),
            Err(e) => {
                error!(error = ?e, "Failed to acquire read lock");
                None
            }
        }
    }

    /// Load providers from API and update the store.
    pub async fn load(api: Arc<Api>) -> Result<()> {
        match api.get_providers().await {
            Ok(providers) => match Self::global().write() {
                Ok(mut p) => p.push(providers),
                Err(e) => error!(error = ?e, "Failed to acquire write lock"),
            },
            Err(e) => return Err(e),
        }

        Ok(())
    }

    /// Health check and reload providers.
    pub async fn health_check_and_reload(api: Arc<Api>, name: &str) -> Result<()> {
        match api.health_check_provider(name).await {
            Ok(_) => Self::load(api).await,
            Err(e) => {
                error!(error = ?e, "Failed to update proxy providers");
                Err(e)
            }
        }
    }

    /// Update provider and reload providers.
    pub async fn update_and_reload(api: Arc<Api>, name: &str) -> Result<()> {
        match api.update_provider(name).await {
            Ok(_) => Self::load(api).await,
            Err(e) => {
                error!(error = ?e, "Failed to update proxy providers");
                Err(e)
            }
        }
    }

    pub fn init_sort_config(sort: Option<ProxySortConfig>) {
        let mut p = Self::global().write().expect("proxy providers store poisoned");
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
            let mut p = Self::global().write().expect("proxy providers store poisoned");
            let old_sort = p.sort.take();
            let new_sort = f(old_sort.clone());
            if old_sort.is_none() && new_sort.is_none() {
                p.sort = new_sort;
                return;
            }
            info!(old = ?old_sort, new = ?new_sort, "Changed proxy provider detail sort");
            p.sort = new_sort;
        } // release lock

        tokio::task::Builder::new()
            .name("proxy-provider-loader")
            .spawn(async {
                if let Err(e) = Self::load(api).await {
                    error!(error = ?e, "Failed to reload proxy providers after sort change");
                }
            })
            .expect("Failed to spawn proxy providers loader task");
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

impl ProxyProviders {
    fn sort_providers(
        providers: &mut IndexMap<String, ProxyProvider>,
        sort_config: &ProxySortConfig,
    ) {
        match sort_config.field {
            ProxySortField::Name => Self::sort_by_name(providers, sort_config.dir),
            ProxySortField::Latency => Self::sort_by_latency(providers, sort_config.dir),
        }
    }

    fn sort_by_name(providers: &mut IndexMap<String, ProxyProvider>, dir: SortDir) {
        for provider in providers.values_mut() {
            provider.proxies.sort_by(|a, b| match dir {
                SortDir::Asc => a.name.cmp(&b.name),
                SortDir::Desc => b.name.cmp(&a.name),
            });
        }
    }

    fn sort_by_latency(providers: &mut IndexMap<String, ProxyProvider>, dir: SortDir) {
        for provider in providers.values_mut() {
            provider.proxies.sort_by(|a, b| {
                let a_latency = a.history.last().map(|h| h.delay).filter(|d| *d > 0);
                let b_latency = b.history.last().map(|h| h.delay).filter(|d| *d > 0);
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
}

impl ProxyProviders {
    pub fn clear(&mut self) {
        self.providers.clear();
        self.providers.shrink_to_fit();
    }

    pub fn push(&mut self, mut providers: IndexMap<String, ProxyProvider>) {
        let threshold = ProxySetting::global().read().unwrap().threshold;
        if let Some(sort) = &self.sort {
            Self::sort_providers(&mut providers, sort);
        }
        self.providers = providers
            .into_values()
            .filter(|v| v.name != "default" && v.vehicle_type != "Compatible")
            .map(|v| self.build_view(v, threshold))
            .collect();
    }

    fn build_view(&self, mut provider: ProxyProvider, threshold: (u64, u64)) -> Arc<ProviderView> {
        provider.updated_at_str = provider.updated_at.and_then(format_datetime);
        let mut quality_stats = [0; LatencyQuality::COUNT];
        for proxy in provider.proxies.iter_mut() {
            proxy.latency = proxy.history.last().map(|h| h.delay).into();
            let idx: usize = LatencyQuality::from(proxy.latency, threshold).into();
            quality_stats[idx] += 1;
        }
        let usage_percent = provider.subscription_info.as_ref().map(|v| {
            if let (Some(d), Some(u), Some(t)) = (v.download, v.upload, v.total)
                && t > 0
            {
                return ((d + u) as f64) * 100.0 / (t as f64);
            }
            0.0
        });

        Arc::new(ProviderView {
            provider: Arc::new(provider),
            quality_stats: QualityStats::new(quality_stats),
            usage_percent,
        })
    }

    pub fn view(&self) -> Vec<Arc<ProviderView>> {
        self.providers.clone()
    }
}
