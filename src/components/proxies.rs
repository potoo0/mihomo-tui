use std::collections::HashMap;
use std::sync::Arc;

use ratatui::style::Color;

use crate::models::proxy::Proxy;

const THRESHOLD: (i64, i64) = (500, 1000);

#[derive(Debug)]
pub struct ProxyView {
    pub proxy: Arc<Proxy>,
    pub delay: Option<i64>,
    pub quality_stats: [usize; LatencyQuality::COUNT],
}

#[repr(usize)]
#[derive(Debug)]
pub enum LatencyQuality {
    Fast = 0,
    Medium = 1,
    Slow = 2,
    NotConnected = 3,
}

#[derive(Debug, Default)]
pub struct Proxies {
    proxies: HashMap<String, Arc<Proxy>>,
    visible: Vec<Arc<ProxyView>>,
}

impl Proxies {
    pub fn push(&mut self, proxies: HashMap<String, Proxy>) {
        self.proxies = proxies.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();

        let sort_index = self.build_sort_index();

        let mut visible: Vec<Arc<ProxyView>> = self
            .proxies
            .values()
            .filter(|p| !(p.hidden == Some(true) || p.all.as_ref().is_none_or(Vec::is_empty)))
            .map(|v| self.build_proxy_view(v))
            .collect();
        visible.sort_by_key(|v| sort_index.get(&v.proxy.name).copied().unwrap_or(usize::MAX));

        self.visible = visible;
    }

    fn build_proxy_view(&self, proxy: &Arc<Proxy>) -> Arc<ProxyView> {
        let mut delay = None;
        if let Some(ref selected) = proxy.selected
            && let Some(p) = self.node(selected)
        {
            delay = p.history.last().map(|h| h.delay);
        }

        let mut quality_stats = [0; LatencyQuality::COUNT];
        if let Some(ref children) = proxy.all {
            for child in children {
                let child_delay = self.node(child).and_then(|v| v.history.last().map(|h| h.delay));
                let idx: usize = LatencyQuality::from_delay(child_delay).into();
                quality_stats[idx] += 1;
            }
        }

        Arc::new(ProxyView { proxy: Arc::clone(proxy), delay, quality_stats })
    }

    fn build_sort_index(&self) -> HashMap<String, usize> {
        self.proxies
            .get("GLOBAL")
            .and_then(|v| v.all.as_ref())
            .into_iter()
            .flat_map(|v| v.iter())
            .enumerate()
            .map(|(idx, key)| (key.clone(), idx))
            .collect()
    }

    pub fn node(&self, name: &str) -> Option<Arc<Proxy>> {
        match self.proxies.get(name) {
            None => None,
            Some(p) => {
                if let Some(ref inner) = p.selected
                    && p.all.is_some()
                {
                    self.node(inner)
                } else {
                    Some(Arc::clone(p))
                }
            }
        }
    }

    pub fn view(&self) -> Vec<Arc<ProxyView>> {
        self.visible.clone()
    }
}

impl LatencyQuality {
    pub const COUNT: usize = 4;

    pub fn from_delay(delay: Option<i64>) -> Self {
        match delay {
            None => LatencyQuality::NotConnected,
            Some(d) if d <= 0 => LatencyQuality::NotConnected,
            Some(d) if d < THRESHOLD.0 => LatencyQuality::Fast,
            Some(d) if d < THRESHOLD.1 => LatencyQuality::Medium,
            Some(_) => LatencyQuality::Slow,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            LatencyQuality::Fast => Color::Rgb(0, 166, 62),
            LatencyQuality::Medium => Color::Rgb(240, 177, 0),
            LatencyQuality::Slow => Color::Rgb(251, 44, 54),
            LatencyQuality::NotConnected => Color::DarkGray,
        }
    }
}

impl From<LatencyQuality> for usize {
    fn from(value: LatencyQuality) -> Self {
        value as usize
    }
}

impl TryFrom<usize> for LatencyQuality {
    type Error = ();

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(LatencyQuality::Fast),
            1 => Ok(LatencyQuality::Medium),
            2 => Ok(LatencyQuality::Slow),
            3 => Ok(LatencyQuality::NotConnected),
            _ => Err(()),
        }
    }
}
