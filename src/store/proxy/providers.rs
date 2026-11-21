use std::cmp::Ordering;
use std::sync::Arc;

use anyhow::Result;
use indexmap::IndexMap;
use tracing::info;

use super::sorting::{next_sort_field, toggle_sort_direction as toggle_dir};
use super::{ProxyProviderDetailSetting, ProxySetting};
use crate::api::Api;
use crate::components::{ComponentId, PopupId};
use crate::config::{LatencyThreshold, ProxySortConfig};
use crate::models::proxy_provider::ProxyProvider;
use crate::models::sort::{ProxySortField, SortDir};
use crate::store::hub::{StoreDisposition, StoreLifecycle, StoreRef};
use crate::ui::latency::{LatencyQuality, QualityStats};
use crate::utils::time::format_datetime;

#[derive(Debug)]
pub struct ProviderView {
    pub provider: Arc<ProxyProvider>,
    pub quality_stats: QualityStats,
    pub usage_percent: Option<f64>,
}

#[derive(Debug)]
pub struct ProxyProviders {
    detail_setting: StoreRef<ProxyProviderDetailSetting>,
    providers: Vec<Arc<ProviderView>>,
}

impl ProxyProviders {
    pub fn new(detail_setting: StoreRef<ProxyProviderDetailSetting>) -> Self {
        Self { detail_setting, providers: Vec::new() }
    }

    pub fn get(&self, index: usize) -> Option<Arc<ProviderView>> {
        self.providers.get(index).cloned()
    }

    pub fn get_by_name(&self, name: &str) -> Option<(usize, Arc<ProviderView>)> {
        self.providers
            .iter()
            .enumerate()
            .find(|(_, view)| view.provider.name == name)
            .map(|(index, view)| (index, Arc::clone(view)))
    }
}

impl StoreLifecycle for ProxyProviders {
    fn disposition() -> StoreDisposition {
        StoreDisposition::RemoveIfNoOwners(&[
            ComponentId::ProxyProviders,
            ComponentId::Popup(PopupId::ProxyProviderDetail),
        ])
    }

    fn on_release(&mut self) {
        self.providers.clear();
    }
}

impl StoreRef<ProxyProviders> {
    pub async fn load(&self, api: Arc<Api>, setting: &StoreRef<ProxySetting>) -> Result<()> {
        let loaded = api.get_providers().await?;
        let threshold = setting.read(|setting| setting.latency_threshold);
        self.update(|store| store.push(loaded, threshold));
        Ok(())
    }

    pub async fn health_check_and_reload(
        &self,
        api: Arc<Api>,
        setting: &StoreRef<ProxySetting>,
        name: &str,
    ) -> Result<()> {
        api.health_check_provider(name).await?;
        self.load(api, setting).await
    }

    pub async fn update_and_reload(
        &self,
        api: Arc<Api>,
        setting: &StoreRef<ProxySetting>,
        name: &str,
    ) -> Result<()> {
        api.update_provider(name).await?;
        self.load(api, setting).await
    }

    pub fn switch_sort_field(&self) -> bool {
        self.update_sort(next_sort_field)
    }

    pub fn toggle_sort_direction(&self) -> bool {
        self.update_sort(toggle_dir)
    }

    fn update_sort<F>(&self, update: F) -> bool
    where
        F: FnOnce(Option<ProxySortConfig>) -> Option<ProxySortConfig>,
    {
        let detail_setting = self.read(|store| store.detail_setting.clone());
        let (old_sort, new_sort) = detail_setting.update(|setting| {
            let old = setting.sort.clone();
            let new_sort = update(old.clone());
            setting.sort = new_sort.clone();
            (old, new_sort)
        });

        if old_sort == new_sort {
            return false;
        }

        info!(old = ?old_sort, new = ?new_sort, "Changed proxy provider detail sort");
        true
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
    pub fn push(
        &mut self,
        mut providers: IndexMap<String, ProxyProvider>,
        threshold: LatencyThreshold,
    ) {
        let sort = self.detail_setting.read(|setting| setting.sort.clone());
        if let Some(sort) = sort {
            Self::sort_providers(&mut providers, &sort);
        }
        self.providers = providers
            .into_values()
            .filter(|v| v.name != "default" && v.vehicle_type != "Compatible")
            .map(|v| self.build_view(v, threshold))
            .collect();
    }

    fn build_view(
        &self,
        mut provider: ProxyProvider,
        threshold: LatencyThreshold,
    ) -> Arc<ProviderView> {
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

    pub fn with_view<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&[Arc<ProviderView>]) -> R,
    {
        f(&self.providers)
    }
}
