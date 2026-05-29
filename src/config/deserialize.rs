use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use super::{ConnectionsUiConfig, LatencyThreshold};
use crate::models::sort::{SortDir, SortSpec};
use crate::store::connections::{
    CONNECTION_COLS, DEFAULT_CONNECTION_COL_INDICES, find_sortable_connection_col,
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
    let columns = raw
        .columns
        .map(raw_connections_columns_into_indices)
        .transpose()?
        .unwrap_or_else(|| DEFAULT_CONNECTION_COL_INDICES.to_vec());
    let sort = raw
        .sort
        .map(raw_connections_sort_into_sort_spec)
        .transpose()?
        .filter(|sort| columns.contains(&sort.col));

    Ok(ConnectionsUiConfig { columns, sort, source_ip_alias: raw.source_ip_alias })
}

fn raw_connections_sort_into_sort_spec(raw: RawConnectionsSortConfig) -> Result<SortSpec> {
    let Some(col) = find_sortable_connection_col(&raw.field) else {
        bail!(
            "invalid `ui.connections.sort.field`: {:?}, allowed values: [{}]",
            raw.field,
            allowed_sortable_connection_col_titles()
        );
    };

    Ok(SortSpec { col, dir: raw.dir })
}

fn raw_connections_columns_into_indices(raw: Vec<String>) -> Result<Vec<usize>> {
    if raw.is_empty() {
        bail!(
            "`ui.connections.columns` cannot be empty, allowed values: [{}]",
            allowed_connection_col_titles()
        );
    }

    let mut cols = Vec::with_capacity(raw.len());
    for field in raw {
        let Some(col) = find_connection_col(&field) else {
            bail!(
                "invalid `ui.connections.columns` value: {:?}, allowed values: [{}]",
                field,
                allowed_connection_col_titles()
            );
        };

        if cols.contains(&col) {
            bail!("duplicate `ui.connections.columns` value: {:?}", field);
        }
        cols.push(col);
    }

    Ok(cols)
}

fn find_connection_col(field: &str) -> Option<usize> {
    CONNECTION_COLS
        .iter()
        .enumerate()
        .find(|(_, def)| def.title.eq_ignore_ascii_case(field))
        .map(|(idx, _)| idx)
}

fn allowed_connection_col_titles() -> String {
    CONNECTION_COLS.iter().map(|def| def.title).collect::<Vec<_>>().join(", ")
}

fn allowed_sortable_connection_col_titles() -> String {
    CONNECTION_COLS
        .iter()
        .filter(|def| def.sortable)
        .map(|def| def.title)
        .collect::<Vec<_>>()
        .join(", ")
}
