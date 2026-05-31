use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use super::{ConnectionsUiConfig, LatencyThreshold};
use crate::models::sort::{SortDir, SortSpec};
use crate::store::connections::{
    ALIVE_COLUMN_INDEX, CONNECTION_COLS, DEFAULT_CONNECTION_COL_INDICES, with_alive_column,
};

impl fmt::Display for LatencyThreshold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{}", self.medium, self.high)
    }
}

impl FromStr for LatencyThreshold {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = value.split(',').collect();
        if parts.len() != 2 {
            bail!("Threshold must be two comma-separated numbers (e.g. 500,1000)");
        }

        let medium = parts[0]
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("Threshold values must be valid positive numbers"))?;
        let high = parts[1]
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("Threshold values must be valid positive numbers"))?;

        Ok(Self { medium, high })
    }
}

impl<'de> Deserialize<'de> for LatencyThreshold {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawConnectionsSortConfig {
    field: String,

    #[serde(default)]
    dir: SortDir,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawConnectionsUiConfig {
    columns: Option<Vec<String>>,

    #[serde(default)]
    sort: Option<RawConnectionsSortConfig>,

    #[serde(default)]
    source_ip_alias: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for ConnectionsUiConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawConnectionsUiConfig::deserialize(deserializer)?;
        raw_connections_ui_config_into_config(raw).map_err(D::Error::custom)
    }
}

fn raw_connections_ui_config_into_config(
    raw: RawConnectionsUiConfig,
) -> Result<ConnectionsUiConfig> {
    let columns = with_alive_column(
        raw.columns
            .map(raw_connections_columns_into_indices)
            .transpose()?
            .unwrap_or_else(|| DEFAULT_CONNECTION_COL_INDICES.to_vec()),
    );

    let sort = raw.sort.map(raw_connections_sort_into_sort_spec).transpose()?.and_then(|sort| {
        // Convert the resolved column definition index into a visible columns index.
        columns.iter().position(|&col| col == sort.col).map(|col| SortSpec { col, dir: sort.dir })
    });

    Ok(ConnectionsUiConfig { columns, sort, source_ip_alias: raw.source_ip_alias })
}

fn raw_connections_sort_into_sort_spec(raw: RawConnectionsSortConfig) -> Result<SortSpec> {
    let sortable_cols = sortable_connection_cols();
    let Some(col) = find_index_ignore_case(&sortable_cols, &raw.field) else {
        bail!(
            "`ui.connections.sort.field` must be one of [{}], got {:?}",
            join_connection_col_titles(&sortable_cols),
            raw.field
        );
    };

    Ok(SortSpec { col, dir: raw.dir })
}

fn raw_connections_columns_into_indices(raw: Vec<String>) -> Result<Vec<usize>> {
    let configurable_cols = configurable_connection_cols();
    if raw.is_empty() {
        bail!(
            "`ui.connections.columns` cannot be empty, must be one of [{}]",
            join_connection_col_titles(&configurable_cols)
        );
    }

    let mut cols = Vec::with_capacity(raw.len());
    for field in raw {
        let Some(col) = find_index_ignore_case(&configurable_cols, &field) else {
            bail!(
                "`ui.connections.columns` values must be one of [{}], got {:?}",
                join_connection_col_titles(&configurable_cols),
                field
            );
        };

        if cols.contains(&col) {
            bail!("duplicate `ui.connections.columns` value: {:?}", field);
        }
        cols.push(col);
    }

    Ok(cols)
}

fn find_index_ignore_case(items: &[(usize, &'static str)], name: &str) -> Option<usize> {
    items.iter().find(|(_, title)| title.eq_ignore_ascii_case(name)).map(|(idx, _)| *idx)
}

fn sortable_connection_cols() -> Vec<(usize, &'static str)> {
    CONNECTION_COLS
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != ALIVE_COLUMN_INDEX)
        .filter(|(_, def)| def.sortable)
        .map(|(idx, def)| (idx, def.title))
        .collect::<Vec<_>>()
}

fn configurable_connection_cols() -> Vec<(usize, &'static str)> {
    CONNECTION_COLS
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != ALIVE_COLUMN_INDEX)
        .map(|(idx, def)| (idx, def.title))
        .collect::<Vec<_>>()
}

fn join_connection_col_titles(cols: &[(usize, &'static str)]) -> String {
    cols.iter().map(|(_, title)| *title).collect::<Vec<_>>().join(", ")
}
