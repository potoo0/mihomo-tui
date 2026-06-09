use std::collections::HashMap;
use std::num::NonZeroUsize;

use serde::Deserialize;
use url::Url;

use crate::models::sort::{ProxySortField, SortDir};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub mihomo_api: Url,
    pub mihomo_secret: Option<String>,
    pub mihomo_config_schema: Option<String>,
    #[serde(default = "default_mihomo_repo")]
    pub mihomo_repo: String,

    pub log_file: Option<String>,

    /// Log filtering directives compatible with `tracing_subscriber::EnvFilter`.
    /// This field accepts the same syntax as `RUST_LOG`, for example:
    ///
    /// - `"info"` — set the global log level
    /// - `"info,mihomo_tui=trace"` — global `info`, override `mihomo_tui` to `trace`
    /// - `"mihomo_tui::api=debug"` — enable logs only for a specific module
    pub log_level: Option<String>,

    pub ui: Option<UiConfig>,

    #[serde(default)]
    pub proxy_setting: ProxySetting,

    #[serde(default)]
    pub buffer: BufferConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct UiConfig {
    pub connections: Option<ConnectionsUiConfig>,
    pub proxy_detail: Option<ProxyDetailUiConfig>,
    pub proxy_provider_detail: Option<ProxyDetailUiConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConnectionsUiConfig {
    pub columns: Option<Vec<String>>,
    pub sort: Option<ConnectionsSortConfig>,
    #[serde(default)]
    pub source_ip_alias: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConnectionsSortConfig {
    pub field: String,

    #[serde(default)]
    pub dir: SortDir,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProxyDetailUiConfig {
    pub sort: Option<ProxySortConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProxySortConfig {
    pub field: ProxySortField,

    #[serde(default = "default_proxy_detail_sort_dir")]
    pub dir: SortDir,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct ProxySetting {
    pub test_url: String,
    pub test_timeout: NonZeroUsize,
    pub latency_threshold: LatencyThreshold,
    pub auto_terminate_connections: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyThreshold {
    pub medium: u64,
    pub high: u64,
}

impl LatencyThreshold {
    pub const fn as_tuple(self) -> (u64, u64) {
        (self.medium, self.high)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct BufferConfig {
    pub overview: OverviewBufferConfig,
    pub connections: NonZeroUsize,
    pub logs: NonZeroUsize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct OverviewBufferConfig {
    pub memory: NonZeroUsize,
    pub traffic: NonZeroUsize,
}

impl Default for LatencyThreshold {
    fn default() -> Self {
        Self { medium: 500, high: 1000 }
    }
}

impl Default for ProxySetting {
    fn default() -> Self {
        Self {
            test_url: "https://www.gstatic.com/generate_204".into(),
            test_timeout: NonZeroUsize::new(5000).unwrap(),
            latency_threshold: LatencyThreshold::default(),
            auto_terminate_connections: false,
        }
    }
}

impl Default for BufferConfig {
    fn default() -> Self {
        BufferConfig {
            overview: Default::default(),
            connections: NonZeroUsize::new(500).unwrap(),
            logs: NonZeroUsize::new(500).unwrap(),
        }
    }
}

impl Default for OverviewBufferConfig {
    fn default() -> Self {
        OverviewBufferConfig {
            memory: NonZeroUsize::new(100).unwrap(),
            traffic: NonZeroUsize::new(100).unwrap(),
        }
    }
}

fn default_proxy_detail_sort_dir() -> SortDir {
    SortDir::Asc
}

pub fn default_mihomo_repo() -> String {
    "MetaCubeX/mihomo".to_owned()
}
