use std::collections::{BTreeMap, HashMap};
use std::num::{NonZeroU16, NonZeroUsize};

use anyhow::{Result, anyhow, bail};
use url::Url;

use crate::config::{
    Config, ConnectionsSortConfig, ConnectionsUiConfig, LatencyThreshold, ProxySetting,
};
use crate::models::sort::SortSpec;
use crate::store::connections::{ALIVE_COLUMN_INDEX, CONNECTION_COLS};

impl Config {
    pub fn validate(&self) -> Result<()> {
        match &self.mihomo_api {
            #[cfg(not(unix))]
            crate::config::MihomoApiEndpoint::UnixSocket(_) => {
                bail!("Unix socket mihomo API is not supported on this platform");
            }
            #[cfg(not(windows))]
            crate::config::MihomoApiEndpoint::WindowsNamedPipe(_) => {
                bail!("Windows named pipe mihomo API is not supported on this platform");
            }
            _ => {}
        }
        self.proxy_setting.validate()?;
        if let Some(connections) = self.ui.as_ref().and_then(|ui| ui.connections.as_ref()) {
            connections.validate()?;
        }
        Ok(())
    }
}

impl ConnectionsUiConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(columns) = &self.columns {
            Self::parse_connections_columns(columns)?;
        }
        if let Some(sort) = &self.sort {
            Self::parse_connections_sort(sort)?;
        }
        Self::parse_connections_column_widths(&self.column_widths)?;
        Ok(())
    }

    pub fn parse_connections_sort(raw: &ConnectionsSortConfig) -> Result<SortSpec> {
        let sortable_cols = Self::sortable_connection_cols();
        let Some(col) = Self::find_index_ignore_case(&sortable_cols, &raw.field) else {
            bail!(
                "`ui.connections.sort.field` must be one of [{}], got {:?}",
                Self::join_connection_col_titles(&sortable_cols),
                raw.field
            );
        };

        Ok(SortSpec { col, dir: raw.dir })
    }

    pub fn parse_connections_columns(raw: &[String]) -> Result<Vec<usize>> {
        let configurable_cols = Self::configurable_connection_cols();
        if raw.is_empty() {
            bail!(
                "`ui.connections.columns` cannot be empty, must be one of [{}]",
                Self::join_connection_col_titles(&configurable_cols)
            );
        }

        let mut cols = Vec::with_capacity(raw.len());
        for field in raw {
            let Some(col) = Self::find_index_ignore_case(&configurable_cols, field) else {
                bail!(
                    "`ui.connections.columns` values must be one of [{}], got {:?}",
                    Self::join_connection_col_titles(&configurable_cols),
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

    pub fn parse_connections_column_widths(
        raw: &BTreeMap<String, NonZeroU16>,
    ) -> Result<HashMap<usize, u16>> {
        let configurable_cols = Self::configurable_connection_cols();
        let mut widths = HashMap::with_capacity(raw.len());
        for (field, width) in raw {
            let Some(col) = Self::find_index_ignore_case(&configurable_cols, field) else {
                bail!(
                    "`ui.connections.column-widths` keys must be one of [{}], got {:?}",
                    Self::join_connection_col_titles(&configurable_cols),
                    field
                );
            };

            if widths.insert(col, width.get()).is_some() {
                bail!("duplicate `ui.connections.column-widths` key: {:?}", field);
            }
        }

        Ok(widths)
    }

    fn find_index_ignore_case(items: &[(usize, &'static str)], name: &str) -> Option<usize> {
        items.iter().find(|(_, title)| title.eq_ignore_ascii_case(name)).map(|(idx, _)| *idx)
    }

    fn sortable_connection_cols() -> Vec<(usize, &'static str)> {
        CONNECTION_COLS
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != ALIVE_COLUMN_INDEX)
            .filter(|(_, def)| def.col.sortable)
            .map(|(idx, def)| (idx, def.col.title))
            .collect::<Vec<_>>()
    }

    fn configurable_connection_cols() -> Vec<(usize, &'static str)> {
        CONNECTION_COLS
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != ALIVE_COLUMN_INDEX)
            .map(|(idx, def)| (idx, def.col.title))
            .collect::<Vec<_>>()
    }

    fn join_connection_col_titles(cols: &[(usize, &'static str)]) -> String {
        cols.iter().map(|(_, title)| *title).collect::<Vec<_>>().join(", ")
    }
}

impl ProxySetting {
    pub fn validate(&self) -> Result<()> {
        Self::validate_test_url(&self.test_url)?;
        Self::validate_test_timeout(self.test_timeout)?;
        Self::validate_latency_threshold(self.latency_threshold)?;
        Ok(())
    }

    pub fn validate_test_url(value: &str) -> Result<()> {
        if value.is_empty() {
            bail!("URL cannot be empty");
        }
        if !value.starts_with("http://") && !value.starts_with("https://") {
            bail!("URL must start with http:// or https://");
        }

        Url::parse(value).map_err(|e| anyhow!("Invalid URL: {}", e))?;
        Ok(())
    }

    pub fn validate_test_timeout(value: NonZeroUsize) -> Result<()> {
        if value.get() <= 60000 {
            Ok(())
        } else {
            bail!("Timeout must be between 1 and 60000 milliseconds");
        }
    }

    pub fn validate_latency_threshold(value: LatencyThreshold) -> Result<()> {
        if value.medium == 0 || value.high == 0 {
            bail!("Threshold values must be valid positive numbers");
        }
        if value.medium >= value.high {
            bail!("Threshold must satisfy medium < high");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_threshold_parse() {
        assert_eq!(
            "500,1000".parse::<LatencyThreshold>().unwrap(),
            LatencyThreshold { medium: 500, high: 1000 }
        );
    }

    #[test]
    fn test_latency_threshold_invalid_order() {
        let err =
            ProxySetting::validate_latency_threshold(LatencyThreshold { medium: 1000, high: 500 })
                .unwrap_err();
        assert!(err.to_string().contains("Threshold must satisfy medium < high"));
    }

    #[test]
    fn test_proxy_test_timeout_range() {
        assert!(ProxySetting::validate_test_timeout(NonZeroUsize::new(1).unwrap()).is_ok());
        assert!(ProxySetting::validate_test_timeout(NonZeroUsize::new(60000).unwrap()).is_ok());
        assert!(ProxySetting::validate_test_timeout(NonZeroUsize::new(60001).unwrap()).is_err());
    }
}
