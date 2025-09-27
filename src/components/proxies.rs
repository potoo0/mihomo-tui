use std::collections::HashMap;
use std::sync::Arc;

use crate::models::proxy::Proxy;
use crate::widgets::latency::LatencyQuality;

#[derive(Debug)]
pub struct ProxyView {
    pub proxy: Arc<Proxy>,
    pub quality_stats: [usize; LatencyQuality::COUNT],
}

#[derive(Debug, Default)]
pub struct Proxies {
    proxies: HashMap<String, Arc<Proxy>>,
    visible: Vec<Arc<ProxyView>>,
}

impl Proxies {
    pub fn push(&mut self, mut proxies: HashMap<String, Proxy>) {
        self.update_delay(&mut proxies);
        self.proxies = proxies.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();

        let sort_index = self.build_sort_index();
        let mut visible: Vec<Arc<ProxyView>> = self
            .proxies
            .values()
            .filter(|p| !(p.hidden == Some(true) || p.children.as_ref().is_none_or(Vec::is_empty)))
            .map(|v| self.build_proxy_view(v))
            .collect();
        visible.sort_by_key(|v| sort_index.get(&v.proxy.name).copied().unwrap_or(usize::MAX));

        self.visible = visible;
    }

    fn build_proxy_view(&self, proxy: &Arc<Proxy>) -> Arc<ProxyView> {
        let mut quality_stats = [0; LatencyQuality::COUNT];
        if let Some(ref children) = proxy.children {
            for child in children {
                let quality: LatencyQuality =
                    self.proxies.get(child).map(|v| v.latency).unwrap_or_default().into();
                let idx: usize = quality.into();
                quality_stats[idx] += 1;
            }
        }

        Arc::new(ProxyView { proxy: Arc::clone(proxy), quality_stats })
    }

    fn update_delay(&mut self, proxies: &mut HashMap<String, Proxy>) {
        fn update(key: &str, proxies: &mut HashMap<String, Proxy>) {
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

    pub fn view(&self) -> Vec<Arc<ProxyView>> {
        self.visible.clone()
    }

    pub fn children(&self, proxy: &Proxy) -> Vec<Arc<Proxy>> {
        proxy
            .children
            .as_ref()
            .into_iter()
            .flat_map(|v| v.iter())
            .filter_map(|v| self.proxies.get(v).cloned())
            .collect()
    }

    pub fn get(&self, index: usize) -> Option<Arc<ProxyView>> {
        self.visible.get(index).cloned()
    }
}
