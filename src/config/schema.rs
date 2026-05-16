use std::num::NonZeroUsize;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

use crate::models::sort::{ProxyGroupSortField, SortDir, SortSpec};
use crate::store::connections::{CONNECTION_COLS, find_sortable_connection_col};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub mihomo_api: Url,
    pub mihomo_secret: Option<String>,
    pub mihomo_config_schema: Option<String>,

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
    pub buffer: BufferConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct UiConfig {
    pub connections: Option<ConnectionsUiConfig>,
    pub proxy_detail: Option<ProxyDetailUiConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConnectionsUiConfig {
    #[serde(default, deserialize_with = "deserialize_connections_sort")]
    pub sort: Option<SortSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProxyDetailUiConfig {
    pub sort: Option<ProxyDetailSortConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProxyDetailSortConfig {
    pub field: ProxyGroupSortField,

    #[serde(default = "default_proxy_detail_sort_dir")]
    pub dir: SortDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct BufferConfig {
    pub overview: OverviewBufferConfig,
    pub connections: NonZeroUsize,
    pub logs: NonZeroUsize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct OverviewBufferConfig {
    pub memory: NonZeroUsize,
    pub traffic: NonZeroUsize,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawConnectionsSortConfig {
    field: String,

    #[serde(default)]
    dir: SortDir,
}

fn deserialize_connections_sort<'de, D>(deserializer: D) -> Result<Option<SortSpec>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<RawConnectionsSortConfig>::deserialize(deserializer)?;
    raw.map(raw_connections_sort_into_sort_spec).transpose().map_err(D::Error::custom)
}

fn raw_connections_sort_into_sort_spec(raw: RawConnectionsSortConfig) -> anyhow::Result<SortSpec> {
    let Some(col) = find_sortable_connection_col(&raw.field) else {
        let allowed = CONNECTION_COLS
            .iter()
            .filter(|def| def.sortable)
            .map(|def| def.title)
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!(
            "invalid `ui.connections.sort.field`: {:?}, allowed values: {}",
            raw.field,
            allowed
        );
    };

    Ok(SortSpec { col, dir: raw.dir })
}
