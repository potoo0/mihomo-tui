use std::sync::Arc;

use indexmap::IndexMap;

use crate::components::proxy_setting::get_proxy_setting;
use crate::models::proxy_provider::ProxyProvider;
use crate::utils::time::format_datetime;
use crate::widgets::latency::{LatencyQuality, QualityStats};

#[derive(Debug)]
pub struct ProviderView {
    pub provider: Arc<ProxyProvider>,
    pub quality_stats: QualityStats,
    pub usage_percent: Option<f64>,
}

#[derive(Debug, Default)]
pub struct ProxyProviders {
    providers: Vec<Arc<ProviderView>>,
}

impl ProxyProviders {
    pub fn push(&mut self, providers: IndexMap<String, ProxyProvider>) {
        let threshold = get_proxy_setting().read().unwrap().threshold;
        self.providers = providers
            .into_values()
            .filter(|v| v.name != "default" && v.vehicle_type != "Compatible")
            .map(|mut v| {
                v.updated_at_str = v.updated_at.and_then(format_datetime);
                v
            })
            .map(|v| self.build_view(v, threshold))
            .collect();
    }

    fn build_view(&self, mut provider: ProxyProvider, threshold: (u64, u64)) -> Arc<ProviderView> {
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

    pub fn get(&self, index: usize) -> Option<Arc<ProviderView>> {
        self.providers.get(index).cloned()
    }
}
